use boltffi_ast::ClassDef as SourceClass;

use crate::{CanonicalName, ClassDecl};

use super::{
    LowerError,
    ids::DeclarationIds,
    index::Index,
    metadata, methods,
    surface::SurfaceLower,
    symbol::{SymbolAllocator, class_release_symbol_name},
};

/// Lowers every class in the source contract.
///
/// `allocator` is shared across the whole pass so each class's
/// release symbol and each method/initializer symbol receives a
/// unique [`SymbolId`] inside the [`Bindings<S>`] under construction.
///
/// [`SymbolId`]: crate::SymbolId
/// [`Bindings<S>`]: crate::Bindings
pub(super) fn lower<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
) -> Result<Vec<ClassDecl<S>>, LowerError> {
    idx.classes()
        .iter()
        .map(|class| lower_one(idx, ids, allocator, class))
        .collect()
}

fn lower_one<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    class: &SourceClass,
) -> Result<ClassDecl<S>, LowerError> {
    let class_id = ids.class(&class.id)?;
    let canonical = CanonicalName::from(&class.name);
    let release = allocator.mint(class_release_symbol_name(class.id.as_str()))?;
    let initializers = methods::lower_class_initializers::<S>(idx, ids, allocator, class)?;
    let class_methods = methods::lower_class_methods::<S>(idx, ids, allocator, class)?;

    Ok(ClassDecl::new(
        class_id,
        canonical,
        metadata::decl_meta(class.doc.as_ref(), class.deprecated.as_ref()),
        S::class_handle_carrier(),
        release,
        initializers,
        class_methods,
    ))
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        CanonicalName as SourceName, ClassDef, DeprecationInfo as SourceDeprecationInfo,
        DocComment as SourceDocComment, EnumDef, FieldDef, MethodDef, MethodId as SourceMethodId,
        PackageInfo as SourcePackage, ParameterDef, Primitive, Receiver, RecordDef, ReturnDef,
        SourceContract, TypeExpr, VariantDef,
    };

    use crate::lower::lower;
    use crate::{
        BindingErrorKind, Bindings, CanonicalName, ClassDecl, ClassId, CodecNode, Decl, EnumId,
        ErrorDecl, ExecutionDecl, HandleTarget, InitializerId, LiftPlan, LowerError,
        LowerErrorKind, LowerPlan, MethodId, Native, NativeSymbol, Primitive as BindingPrimitive,
        Receive, RecordId, ReturnTypeRef, Surface, SurfaceLower, TypeRef, UnsupportedType, Wasm32,
        native, wasm32,
    };

    fn package() -> SourceContract {
        SourceContract::new(SourcePackage::new("demo", Some("0.1.0".to_owned())))
    }

    fn name(part: &str) -> SourceName {
        SourceName::single(part)
    }

    fn class(id: &str, class_name: &str, methods: Vec<MethodDef>) -> ClassDef {
        let mut class = ClassDef::new(id.into(), name(class_name));
        class.methods = methods;
        class
    }

    fn method(method_name: &str, receiver: Receiver, returns: ReturnDef) -> MethodDef {
        let mut method = MethodDef::new(
            SourceMethodId::new(method_name.to_owned()),
            name(method_name),
            receiver,
        );
        method.returns = returns;
        method
    }

    fn param(param_name: &str, type_expr: TypeExpr) -> ParameterDef {
        ParameterDef::value(name(param_name), type_expr)
    }

    fn field(field_name: &str, type_expr: TypeExpr) -> FieldDef {
        FieldDef::new(name(field_name), type_expr)
    }

    fn record(id: &str, record_name: &str, fields: Vec<FieldDef>) -> RecordDef {
        let mut record = RecordDef::new(id.into(), name(record_name));
        record.fields = fields;
        record
    }

    fn enumeration(id: &str, enum_name: &str, variants: Vec<VariantDef>) -> EnumDef {
        let mut enumeration = EnumDef::new(id.into(), name(enum_name));
        enumeration.variants = variants;
        enumeration
    }

    fn lower_class<S: SurfaceLower>(class: ClassDef) -> Bindings<S> {
        let mut contract = package();
        contract.classes.push(class);
        lower::<S>(&contract).expect("class should lower")
    }

    fn lower_contract<S: SurfaceLower>(
        contract: SourceContract,
    ) -> Result<Bindings<S>, LowerError> {
        lower::<S>(&contract)
    }

    fn class_by_id<S: Surface>(bindings: &Bindings<S>, id: ClassId) -> &ClassDecl<S> {
        bindings
            .decls()
            .iter()
            .find_map(|decl| match decl {
                Decl::Class(class) if class.id() == id => Some(class.as_ref()),
                _ => None,
            })
            .expect("expected class declaration")
    }

    fn symbol_name(symbol: &NativeSymbol) -> &str {
        symbol.name().as_str()
    }

    fn symbol_names<S: Surface>(bindings: &Bindings<S>) -> Vec<&str> {
        bindings
            .symbols()
            .symbols()
            .iter()
            .map(|symbol| symbol.name().as_str())
            .collect()
    }

    #[test]
    fn lowers_class_with_initializer_method_release_and_metadata() {
        let mut new = method("new", Receiver::None, ReturnDef::Value(TypeExpr::SelfType));
        new.parameters
            .push(param("seed", TypeExpr::Primitive(Primitive::U64)));
        let start = method("start", Receiver::Mutable, ReturnDef::Void);
        let mut source = class("demo::Engine", "Engine", vec![new, start]);
        source.doc = Some(SourceDocComment::new("Runs work."));
        source.deprecated = Some(SourceDeprecationInfo::new(
            Some("use Runtime".to_owned()),
            Some("0.24.0".to_owned()),
        ));

        let bindings = lower_class::<Native>(source);
        let class = class_by_id(&bindings, ClassId::from_raw(0));

        assert_eq!(class.id(), ClassId::from_raw(0));
        assert_eq!(class.name(), &CanonicalName::single("Engine"));
        assert_eq!(
            class.meta().doc().map(|doc| doc.as_str()),
            Some("Runs work.")
        );
        assert_eq!(
            class
                .meta()
                .deprecated()
                .and_then(|deprecated| deprecated.message()),
            Some("use Runtime")
        );
        assert_eq!(
            class
                .meta()
                .deprecated()
                .and_then(|deprecated| deprecated.since()),
            Some("0.24.0")
        );
        assert_eq!(class.handle(), native::HandleCarrier::U64);
        assert_eq!(
            symbol_name(class.release()),
            "boltffi_release_class_demo_engine"
        );

        let initializer = class.initializers().first().expect("expected initializer");
        assert_eq!(class.initializers().len(), 1);
        assert_eq!(initializer.id(), InitializerId::from_raw(0));
        assert_eq!(initializer.name(), &CanonicalName::single("new"));
        assert_eq!(
            symbol_name(initializer.symbol()),
            "boltffi_init_class_demo_engine_new"
        );
        assert_eq!(
            initializer.returns(),
            &ReturnTypeRef::Value(TypeRef::Class(ClassId::from_raw(0)))
        );
        assert_eq!(initializer.callable().receiver(), None);
        assert_eq!(
            initializer
                .callable()
                .params()
                .first()
                .map(|param| param.lower()),
            Some(&LowerPlan::Direct {
                ty: TypeRef::Primitive(BindingPrimitive::U64),
                receive: Receive::ByValue,
            })
        );
        assert_eq!(
            initializer.callable().returns().lift(),
            &LiftPlan::Handle {
                target: HandleTarget::Class(ClassId::from_raw(0)),
                carrier: native::HandleCarrier::U64,
            }
        );
        assert!(matches!(initializer.callable().error(), ErrorDecl::None(_)));
        assert!(matches!(
            initializer.callable().execution(),
            ExecutionDecl::Synchronous(_)
        ));

        let class_method = class.methods().first().expect("expected method");
        assert_eq!(class.methods().len(), 1);
        assert_eq!(class_method.id(), MethodId::from_raw(0));
        assert_eq!(class_method.name(), &CanonicalName::single("start"));
        assert_eq!(
            symbol_name(class_method.target()),
            "boltffi_method_class_demo_engine_start"
        );
        assert_eq!(class_method.callable().receiver(), Some(Receive::ByMutRef));
        assert_eq!(class_method.callable().returns().lift(), &LiftPlan::Void);
    }

    #[test]
    fn result_self_initializer_uses_handle_out_and_encoded_error() {
        let try_new = method(
            "try_new",
            Receiver::None,
            ReturnDef::Value(TypeExpr::Result {
                ok: Box::new(TypeExpr::SelfType),
                err: Box::new(TypeExpr::String),
            }),
        );
        let bindings = lower_class::<Native>(class("demo::Engine", "Engine", vec![try_new]));
        let class = class_by_id(&bindings, ClassId::from_raw(0));
        let initializer = class.initializers().first().expect("expected initializer");

        assert_eq!(class.initializers().len(), 1);
        assert!(class.methods().is_empty());
        assert_eq!(
            symbol_name(initializer.symbol()),
            "boltffi_init_class_demo_engine_try_new"
        );
        assert_eq!(
            initializer.callable().returns().lift(),
            &LiftPlan::HandleOut {
                target: HandleTarget::Class(ClassId::from_raw(0)),
                carrier: native::HandleCarrier::U64,
            }
        );
        match initializer.callable().error() {
            ErrorDecl::EncodedReturn {
                ty,
                read,
                shape: native::BufferShape::Buffer,
            } => {
                assert_eq!(ty, &TypeRef::String);
                assert_eq!(read.root(), &CodecNode::String);
            }
            other => panic!("expected encoded string error, got {other:?}"),
        }
    }

    #[test]
    fn keeps_static_non_constructor_as_method() {
        let version = method(
            "version",
            Receiver::None,
            ReturnDef::Value(TypeExpr::Primitive(Primitive::I32)),
        );
        let bindings = lower_class::<Native>(class("demo::Engine", "Engine", vec![version]));
        let class = class_by_id(&bindings, ClassId::from_raw(0));

        assert!(class.initializers().is_empty());
        let class_method = class.methods().first().expect("expected method");
        assert_eq!(class.methods().len(), 1);
        assert_eq!(class_method.callable().receiver(), None);
        assert_eq!(
            symbol_name(class_method.target()),
            "boltffi_method_class_demo_engine_version"
        );
        assert_eq!(
            class_method.callable().returns().lift(),
            &LiftPlan::Direct {
                ty: TypeRef::Primitive(BindingPrimitive::I32),
            }
        );
    }

    #[test]
    fn lowers_self_and_named_class_references_to_exact_handle_targets() {
        let driver = class("demo::Driver", "Driver", Vec::new());
        let mut merge = method(
            "merge",
            Receiver::Shared,
            ReturnDef::Value(TypeExpr::SelfType),
        );
        merge.parameters.push(param("other", TypeExpr::SelfType));
        merge
            .parameters
            .push(param("driver", TypeExpr::Class("demo::Driver".into())));
        let engine = class("demo::Engine", "Engine", vec![merge]);
        let mut contract = package();
        contract.classes = vec![driver, engine];

        let bindings = lower_contract::<Native>(contract).expect("classes should lower");
        let class = class_by_id(&bindings, ClassId::from_raw(1));
        let class_method = class.methods().first().expect("expected method");

        assert_eq!(
            class_method
                .callable()
                .params()
                .first()
                .map(|param| param.lower()),
            Some(&LowerPlan::Handle {
                target: HandleTarget::Class(ClassId::from_raw(1)),
                carrier: native::HandleCarrier::U64,
                receive: Receive::ByValue,
            })
        );
        assert_eq!(
            class_method
                .callable()
                .params()
                .get(1)
                .map(|param| param.lower()),
            Some(&LowerPlan::Handle {
                target: HandleTarget::Class(ClassId::from_raw(0)),
                carrier: native::HandleCarrier::U64,
                receive: Receive::ByValue,
            })
        );
        assert_eq!(
            class_method.callable().returns().lift(),
            &LiftPlan::Handle {
                target: HandleTarget::Class(ClassId::from_raw(1)),
                carrier: native::HandleCarrier::U64,
            }
        );
    }

    #[test]
    fn lowers_wasm32_class_handles_to_surface_carrier() {
        let mut clone = method(
            "clone_handle",
            Receiver::Shared,
            ReturnDef::Value(TypeExpr::SelfType),
        );
        clone.parameters.push(param("other", TypeExpr::SelfType));
        let bindings = lower_class::<Wasm32>(class("demo::Engine", "Engine", vec![clone]));
        let class = class_by_id(&bindings, ClassId::from_raw(0));
        let class_method = class.methods().first().expect("expected method");

        assert_eq!(class.handle(), wasm32::HandleCarrier::U32);
        assert_eq!(
            class_method
                .callable()
                .params()
                .first()
                .map(|param| param.lower()),
            Some(&LowerPlan::Handle {
                target: HandleTarget::Class(ClassId::from_raw(0)),
                carrier: wasm32::HandleCarrier::U32,
                receive: Receive::ByValue,
            })
        );
        assert_eq!(
            class_method.callable().returns().lift(),
            &LiftPlan::Handle {
                target: HandleTarget::Class(ClassId::from_raw(0)),
                carrier: wasm32::HandleCarrier::U32,
            }
        );
    }

    #[test]
    fn registers_release_initializer_and_method_symbols_in_order() {
        let new = method("new", Receiver::None, ReturnDef::Value(TypeExpr::SelfType));
        let start = method("start", Receiver::Shared, ReturnDef::Void);
        let bindings = lower_class::<Native>(class("demo::Engine", "Engine", vec![new, start]));

        assert_eq!(
            symbol_names(&bindings),
            vec![
                "boltffi_release_class_demo_engine",
                "boltffi_init_class_demo_engine_new",
                "boltffi_method_class_demo_engine_start",
            ]
        );
    }

    #[test]
    fn class_method_named_free_uses_method_lane_not_release_lane() {
        let free = method("free", Receiver::Shared, ReturnDef::Void);
        let bindings = lower_class::<Native>(class("demo::Engine", "Engine", vec![free]));
        let class = class_by_id(&bindings, ClassId::from_raw(0));
        let method = class.methods().first().expect("expected method");

        assert_eq!(
            symbol_name(class.release()),
            "boltffi_release_class_demo_engine"
        );
        assert_eq!(
            symbol_name(method.target()),
            "boltffi_method_class_demo_engine_free"
        );
    }

    #[test]
    fn same_leaf_class_names_use_source_id_in_symbols() {
        let bindings = lower_contract::<Native>({
            let mut contract = package();
            contract
                .classes
                .push(class("demo::audio::Engine", "Engine", Vec::new()));
            contract
                .classes
                .push(class("demo::video::Engine", "Engine", Vec::new()));
            contract
        })
        .expect("same leaf names should lower");

        assert_eq!(
            symbol_names(&bindings),
            vec![
                "boltffi_release_class_demo_audio_engine",
                "boltffi_release_class_demo_video_engine",
            ]
        );
    }

    #[test]
    fn duplicate_class_method_symbols_fail_with_exact_name() {
        let error = lower_contract::<Native>({
            let mut contract = package();
            contract.classes.push(class(
                "demo::Engine",
                "Engine",
                vec![
                    method("start", Receiver::Shared, ReturnDef::Void),
                    method("start", Receiver::Mutable, ReturnDef::Void),
                ],
            ));
            contract
        })
        .expect_err("duplicate method symbols should fail");

        match error.kind() {
            LowerErrorKind::InvalidBindings(binding_error) => {
                assert!(matches!(
                    binding_error.kind(),
                    BindingErrorKind::DuplicateSymbolName(name)
                        if name == "boltffi_method_class_demo_engine_start"
                ));
            }
            other => panic!("expected invalid bindings, got {other:?}"),
        }
    }

    #[test]
    fn rejects_owned_class_receiver() {
        let consume = method("consume", Receiver::Owned, ReturnDef::Void);
        let error = lower_contract::<Native>({
            let mut contract = package();
            contract
                .classes
                .push(class("demo::Engine", "Engine", vec![consume]));
            contract
        })
        .expect_err("owned receiver should be rejected");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::OwnedClassReceiver)
        ));
    }

    #[test]
    fn class_methods_reference_record_and_enum_ids_exactly() {
        let point = record(
            "demo::Point",
            "Point",
            vec![
                field("x", TypeExpr::Primitive(Primitive::F64)),
                field("y", TypeExpr::Primitive(Primitive::F64)),
            ],
        );
        let direction = enumeration(
            "demo::Direction",
            "Direction",
            vec![
                VariantDef::unit(name("North")),
                VariantDef::unit(name("South")),
            ],
        );
        let locate = method(
            "locate",
            Receiver::Shared,
            ReturnDef::Value(TypeExpr::Record("demo::Point".into())),
        );
        let direction_method = method(
            "direction",
            Receiver::Shared,
            ReturnDef::Value(TypeExpr::Enum("demo::Direction".into())),
        );
        let mut contract = package();
        contract.records.push(point);
        contract.enums.push(direction);
        contract.classes.push(class(
            "demo::Engine",
            "Engine",
            vec![locate, direction_method],
        ));

        let bindings = lower_contract::<Native>(contract).expect("contract should lower");
        let class = class_by_id(&bindings, ClassId::from_raw(0));

        assert_eq!(
            class
                .methods()
                .first()
                .map(|method| method.callable().returns().lift()),
            Some(&LiftPlan::Direct {
                ty: TypeRef::Record(RecordId::from_raw(0)),
            })
        );
        assert_eq!(
            class
                .methods()
                .get(1)
                .map(|method| method.callable().returns().lift()),
            Some(&LiftPlan::Direct {
                ty: TypeRef::Enum(EnumId::from_raw(0)),
            })
        );
    }
}

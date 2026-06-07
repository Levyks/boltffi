//! Callback-trait lowering.
//!
//! Callback traits invert ownership: foreign code provides the methods,
//! Rust calls them through a per-surface dispatch table. Native dispatches
//! through a vtable struct whose slots carry function pointers, so each
//! method maps to a [`VTableSlot`]. Wasm32 has no vtable struct; each
//! dispatch slot is its own imported function in the wasm module, so each
//! method maps to an [`ImportSymbol`].
//!
//! The shape of the resulting [`S::CallbackProtocol`] is therefore
//! surface-divergent. Rather than leaking that decision into the public
//! [`SurfaceLower`] trait, [`CallbackProtocolBuilder`] is a sealed
//! extension trait private to this module. The public [`super::lower`]
//! function carries the private bound under `#[allow(private_bounds)]`
//! so callers only see the [`SurfaceLower`] contract.

use boltffi_ast::{ExecutionKind, MethodDef, TraitDef as SourceTrait};

use crate::{
    CallbackDecl, CanonicalName, ExecutionDecl, ImportModule, ImportSymbol, Native, Surface,
    SymbolName, VTableSlot, Wasm32, native, wasm32,
};

use super::{
    LowerError,
    error::UnsupportedType,
    ids::DeclarationIds,
    index::Index,
    metadata, methods,
    surface::SurfaceLower,
    symbol::{
        self, CallbackSlot, SymbolAllocator, VTABLE_CLONE_SLOT_NAME, VTABLE_FREE_SLOT_NAME,
        WASM_CALLBACK_IMPORT_MODULE,
    },
};

/// Lowers every callback trait the source declares.
///
/// The `CallbackProtocolBuilder` extension lives behind [`SurfaceLower`]'s
/// sealed private supertrait set, so the `S: SurfaceLower` bound is the only
/// constraint callers need to satisfy.
pub(super) fn lower<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
) -> Result<Vec<CallbackDecl<S>>, LowerError> {
    idx.traits()
        .iter()
        .map(|callback| lower_one::<S>(idx, ids, allocator, callback))
        .collect()
}

fn lower_one<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    callback: &SourceTrait,
) -> Result<CallbackDecl<S>, LowerError> {
    reject_slot_collisions(callback)?;
    let callback_id = ids.callback(&callback.id)?;
    let canonical = CanonicalName::from(&callback.name);
    let protocol = S::build_callback_protocol(idx, ids, allocator, callback)?;
    Ok(CallbackDecl::new(
        callback_id,
        canonical,
        metadata::decl_meta(callback.doc.as_ref(), callback.deprecated.as_ref()),
        S::callback_handle_carrier(),
        protocol,
    ))
}

fn reject_slot_collisions(callback: &SourceTrait) -> Result<(), LowerError> {
    let mut seen: Vec<CallbackSlot> = Vec::with_capacity(callback.methods.len());
    callback.methods.iter().try_for_each(|method| {
        let raw = method.name.parts().last().map_or("", |part| part.as_str());
        let slot = CallbackSlot::from_method_name(raw);
        let collides_with_lifecycle =
            slot.as_str() == VTABLE_FREE_SLOT_NAME || slot.as_str() == VTABLE_CLONE_SLOT_NAME;
        let collides_with_peer = seen.iter().any(|existing| existing == &slot);
        if collides_with_lifecycle || collides_with_peer {
            return Err(LowerError::unsupported_type(
                UnsupportedType::CallbackMethodSlotCollision,
            ));
        }
        seen.push(slot);
        Ok(())
    })
}

/// Surface-specific construction of [`Surface::CallbackProtocol`].
///
/// Implemented for [`Native`] and [`Wasm32`] only. Wired in as a private
/// supertrait of [`SurfaceLower`] so the public lowering API stays a
/// shape-picker contract; the protocol constructor is reachable only
/// through the sealed bound.
pub(super) trait CallbackProtocolBuilder: Surface {
    fn build_callback_protocol(
        idx: &Index<'_>,
        ids: &DeclarationIds,
        allocator: &mut SymbolAllocator,
        callback: &SourceTrait,
    ) -> Result<Self::CallbackProtocol, LowerError>;
}

impl CallbackProtocolBuilder for Native {
    fn build_callback_protocol(
        idx: &Index<'_>,
        ids: &DeclarationIds,
        allocator: &mut SymbolAllocator,
        callback: &SourceTrait,
    ) -> Result<Self::CallbackProtocol, LowerError> {
        let register =
            allocator.mint(symbol::callback_register_symbol_name(callback.id.as_str()))?;
        let create_handle = allocator.mint(symbol::callback_create_handle_symbol_name(
            callback.id.as_str(),
        ))?;
        let methods = methods::lower_callback_methods::<Self, VTableSlot, _>(
            idx,
            ids,
            allocator,
            callback,
            |_allocator, method, slot| {
                let target =
                    VTableSlot::parse(slot.as_str().to_owned()).map_err(LowerError::from)?;
                Ok(methods::CallbackMethodSurface::new(
                    target,
                    native_callback_execution(method),
                ))
            },
        )?;
        let vtable = native::CallbackVTable::new(
            VTableSlot::parse(VTABLE_FREE_SLOT_NAME.to_owned())?,
            VTableSlot::parse(VTABLE_CLONE_SLOT_NAME.to_owned())?,
            methods,
        );
        Ok(native::CallbackProtocol::new(
            register,
            create_handle,
            vtable,
        ))
    }
}

impl CallbackProtocolBuilder for Wasm32 {
    fn build_callback_protocol(
        idx: &Index<'_>,
        ids: &DeclarationIds,
        allocator: &mut SymbolAllocator,
        callback: &SourceTrait,
    ) -> Result<Self::CallbackProtocol, LowerError> {
        let module = ImportModule::parse(WASM_CALLBACK_IMPORT_MODULE.to_owned())?;
        let create_handle = allocator.mint(symbol::callback_create_handle_symbol_name(
            callback.id.as_str(),
        ))?;
        let free = wasm_import(
            &module,
            symbol::callback_wasm_import_free_name(callback.id.as_str()),
        )?;
        let clone = wasm_import(
            &module,
            symbol::callback_wasm_import_clone_name(callback.id.as_str()),
        )?;
        let callback_id = callback.id.as_str();
        let methods = methods::lower_callback_methods::<Self, ImportSymbol, _>(
            idx,
            ids,
            allocator,
            callback,
            |allocator, method, slot| {
                wasm_callback_method_surface(allocator, &module, callback_id, method, slot)
            },
        )?;
        Ok(wasm32::CallbackProtocol::new(
            create_handle,
            free,
            clone,
            methods,
        ))
    }
}

fn wasm_import(module: &ImportModule, name: String) -> Result<ImportSymbol, LowerError> {
    Ok(ImportSymbol::new(module.clone(), SymbolName::parse(name)?))
}

fn native_callback_execution(method: &MethodDef) -> ExecutionDecl<Native> {
    match method.execution {
        ExecutionKind::Sync => ExecutionDecl::synchronous(),
        ExecutionKind::Async => {
            ExecutionDecl::asynchronous(native::AsyncProtocol::CallbackCompletion)
        }
    }
}

fn wasm_callback_method_surface(
    allocator: &mut SymbolAllocator,
    module: &ImportModule,
    callback_id: &str,
    method: &MethodDef,
    slot: &CallbackSlot,
) -> Result<methods::CallbackMethodSurface<Wasm32, ImportSymbol>, LowerError> {
    match method.execution {
        ExecutionKind::Sync => Ok(methods::CallbackMethodSurface::new(
            wasm_import(
                module,
                symbol::callback_wasm_import_method_name(callback_id, slot),
            )?,
            ExecutionDecl::synchronous(),
        )),
        ExecutionKind::Async => {
            let target = wasm_import(
                module,
                symbol::callback_wasm_import_start_name(callback_id, slot),
            )?;
            let complete = allocator.mint(symbol::callback_wasm_complete_symbol_name(
                callback_id,
                slot,
            ))?;
            Ok(methods::CallbackMethodSurface::new(
                target,
                ExecutionDecl::asynchronous(wasm32::AsyncProtocol::CallbackCompletion { complete }),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        CanonicalName as SourceName, ClassDef, DeprecationInfo as SourceDeprecationInfo,
        DocComment as SourceDocComment, FieldDef, FnSig, FnTrait, FnTraitKind, MethodDef,
        MethodId as SourceMethodId, PackageInfo as SourcePackage, ParameterDef, ParameterPassing,
        Path, Primitive, Receiver, RecordDef, ReturnDef, SourceContract, TraitDef,
        TraitId as SourceTraitId, TypeExpr,
    };

    use crate::lower::lower;
    use crate::lower::{LowerErrorKind, UnsupportedType};
    use crate::{
        Bindings, CallbackDecl, CodecNode, Decl, ErrorDecl, ExecutionDecl, HandlePresence,
        HandleTarget, Native, ParamPlan, Receive, ReturnPlan, SurfaceLower, TypeRef, ValueRef,
        Wasm32, native, wasm32,
    };

    fn package() -> SourceContract {
        SourceContract::new(SourcePackage::new("demo", Some("0.1.0".to_owned())))
    }

    fn name(part: &str) -> SourceName {
        SourceName::single(part)
    }

    fn listener_callback() -> TraitDef {
        TraitDef::new("demo::Listener".into(), name("Listener"))
    }

    fn impl_listener() -> TypeExpr {
        TypeExpr::impl_trait(
            SourceTraitId::new("demo::Listener"),
            Path::single("Listener"),
        )
    }

    fn boxed_listener() -> TypeExpr {
        TypeExpr::boxed(TypeExpr::dyn_trait(
            SourceTraitId::new("demo::Listener"),
            Path::single("Listener"),
        ))
    }

    fn borrowed_listener_object() -> TypeExpr {
        TypeExpr::dyn_trait(
            SourceTraitId::new("demo::Listener"),
            Path::single("Listener"),
        )
    }

    fn arc_listener() -> TypeExpr {
        TypeExpr::arc(TypeExpr::dyn_trait(
            SourceTraitId::new("demo::Listener"),
            Path::single("Listener"),
        ))
    }

    fn closure() -> TypeExpr {
        closure_returning(ReturnDef::Void)
    }

    fn closure_returning(returns: ReturnDef) -> TypeExpr {
        TypeExpr::impl_fn(FnTrait::new(
            FnTraitKind::Fn,
            FnSig::new(vec![TypeExpr::Primitive(Primitive::U32)], returns),
        ))
    }

    fn boxed_callback_trait(id: &str, path: &str) -> TypeExpr {
        TypeExpr::boxed(TypeExpr::dyn_trait(
            SourceTraitId::new(id),
            Path::single(path),
        ))
    }

    fn nullable(type_expr: TypeExpr) -> TypeExpr {
        TypeExpr::option(type_expr)
    }

    fn method(method_name: &str, receiver: Receiver) -> MethodDef {
        MethodDef::new(
            SourceMethodId::new(method_name),
            name(method_name),
            receiver,
        )
    }

    fn value_param(param_name: &str, type_expr: TypeExpr) -> ParameterDef {
        ParameterDef::value(name(param_name), type_expr)
    }

    fn param(param_name: &str, type_expr: TypeExpr, passing: ParameterPassing) -> ParameterDef {
        let mut parameter = ParameterDef::value(name(param_name), type_expr);
        parameter.passing = passing;
        parameter
    }

    fn lower_callback<S: SurfaceLower>(callback: TraitDef) -> Bindings<S> {
        let mut contract = package();
        contract.traits.push(callback);
        lower::<S>(&contract).expect("callback should lower")
    }

    fn first_callback<S: SurfaceLower>(bindings: &Bindings<S>) -> &CallbackDecl<S> {
        bindings
            .decls()
            .iter()
            .find_map(|decl| match decl {
                Decl::Callback(callback) => Some(callback.as_ref()),
                _ => None,
            })
            .expect("expected callback declaration")
    }

    fn lower_record_with_listener_param<S: SurfaceLower>(
        listener_type: TypeExpr,
    ) -> Result<Bindings<S>, crate::lower::LowerError> {
        lower_record_with_listener_param_passing::<S>(listener_type, ParameterPassing::Value)
    }

    fn lower_record_with_listener_param_passing<S: SurfaceLower>(
        listener_type: TypeExpr,
        passing: ParameterPassing,
    ) -> Result<Bindings<S>, crate::lower::LowerError> {
        let mut contract = package();
        contract.traits.push(listener_callback());
        let mut record = RecordDef::new("demo::Engine".into(), name("Engine"));
        record.fields = vec![FieldDef::new(
            name("ticks"),
            TypeExpr::Primitive(Primitive::U32),
        )];
        let mut install = method("install", Receiver::Mutable);
        install.parameters = vec![param("listener", listener_type, passing)];
        record.methods.push(install);
        contract.records.push(record);
        lower::<S>(&contract)
    }

    fn lower_class_returning_listener<S: SurfaceLower>(
        listener_type: TypeExpr,
    ) -> Result<Bindings<S>, crate::lower::LowerError> {
        let mut contract = package();
        contract.traits.push(listener_callback());
        let mut class = ClassDef::new("demo::Engine".into(), name("Engine"));
        let mut take_listener = method("take_listener", Receiver::Mutable);
        take_listener.returns = ReturnDef::value(listener_type);
        class.methods.push(take_listener);
        contract.classes.push(class);
        lower::<S>(&contract)
    }

    fn record_first_method_lower_plan<S: SurfaceLower>(
        bindings: &Bindings<S>,
    ) -> &crate::ParamPlan<S, crate::IntoRust> {
        let methods = bindings
            .decls()
            .iter()
            .find_map(|decl| match decl {
                Decl::Record(record) => match record.as_ref() {
                    crate::RecordDecl::Direct(direct) => Some(direct.methods()),
                    crate::RecordDecl::Encoded(encoded) => Some(encoded.methods()),
                },
                _ => None,
            })
            .expect("expected record");
        methods[0].callable().params()[0].as_value().unwrap()
    }

    fn class_first_method_lift_plan<S: SurfaceLower>(
        bindings: &Bindings<S>,
    ) -> &crate::ReturnPlan<S, crate::OutOfRust> {
        let methods = bindings
            .decls()
            .iter()
            .find_map(|decl| match decl {
                Decl::Class(class) => Some(class.methods()),
                _ => None,
            })
            .expect("expected class");
        methods[0].callable().returns().plan()
    }

    #[test]
    fn callback_with_no_methods_lowers_with_protocol_only() {
        let bindings = lower_callback::<Native>(listener_callback());
        let callback = first_callback(&bindings);

        assert_eq!(callback.handle(), native::HandleCarrier::CallbackHandle);
        assert_eq!(callback.protocol().vtable().methods().len(), 0);
        assert_eq!(
            callback.protocol().register().name().as_str(),
            "boltffi_register_callback_demo_listener"
        );
        assert_eq!(
            callback.protocol().create_handle().name().as_str(),
            "boltffi_create_callback_demo_listener"
        );
    }

    #[test]
    fn native_callback_vtable_has_free_and_clone_slots() {
        let bindings = lower_callback::<Native>(listener_callback());
        let callback = first_callback(&bindings);
        let vtable = callback.protocol().vtable();

        assert_eq!(vtable.free_slot().as_str(), "free");
        assert_eq!(vtable.clone_slot().as_str(), "clone");
    }

    #[test]
    fn callback_handle_carrier_is_u32_on_wasm32() {
        let bindings = lower_callback::<Wasm32>(listener_callback());
        let callback = first_callback(&bindings);

        assert_eq!(callback.handle(), wasm32::HandleCarrier::U32);
    }

    #[test]
    fn wasm32_callback_protocol_uses_env_imports() {
        let bindings = lower_callback::<Wasm32>(listener_callback());
        let callback = first_callback(&bindings);
        let protocol = callback.protocol();

        assert_eq!(
            protocol.create_handle().name().as_str(),
            "boltffi_create_callback_demo_listener"
        );
        assert_eq!(protocol.free().module().as_str(), "env");
        assert_eq!(
            protocol.free().name().as_str(),
            "__boltffi_callback_lifecycle_demo_listener_free"
        );
        assert_eq!(protocol.clone_import().module().as_str(), "env");
        assert_eq!(
            protocol.clone_import().name().as_str(),
            "__boltffi_callback_lifecycle_demo_listener_clone"
        );
    }

    #[test]
    fn native_callback_method_target_is_a_vtable_slot() {
        let mut callback = listener_callback();
        callback.methods.push(method("on_event", Receiver::Shared));

        let bindings = lower_callback::<Native>(callback);
        let methods = first_callback(&bindings).protocol().vtable().methods();

        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].target().as_str(), "on_event");
        assert_eq!(methods[0].callable().receiver(), Some(Receive::ByRef));
    }

    #[test]
    fn wasm32_callback_method_target_is_an_env_import() {
        let mut callback = listener_callback();
        callback.methods.push(method("on_event", Receiver::Shared));

        let bindings = lower_callback::<Wasm32>(callback);
        let methods = first_callback(&bindings).protocol().methods();

        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].target().module().as_str(), "env");
        assert_eq!(
            methods[0].target().name().as_str(),
            "__boltffi_callback_method_demo_listener_on_event"
        );
    }

    #[test]
    fn callback_method_with_primitive_param_lowers_to_direct_callable() {
        let mut callback = listener_callback();
        let mut handle = method("handle", Receiver::Shared);
        handle.parameters = vec![value_param("code", TypeExpr::Primitive(Primitive::I32))];
        callback.methods.push(handle);

        let bindings = lower_callback::<Native>(callback);
        let methods = first_callback(&bindings).protocol().vtable().methods();
        let params = methods[0].callable().params();

        assert_eq!(params.len(), 1);
        assert!(matches!(
            params[0].as_value().unwrap(),
            ParamPlan::Direct {
                ty: TypeRef::Primitive(crate::Primitive::I32),
                // direction is OutOfRust (Rust pushes args to foreign
                // implementation), so the slot has no Rust-side receive mode
                receive: (),
            }
        ));
    }

    #[test]
    fn callback_method_with_string_param_uses_read_codec() {
        let mut callback = listener_callback();
        let mut handle = method("handle", Receiver::Shared);
        handle.parameters = vec![value_param("message", TypeExpr::String)];
        callback.methods.push(handle);

        let bindings = lower_callback::<Native>(callback);
        let methods = first_callback(&bindings).protocol().vtable().methods();
        let params = methods[0].callable().params();

        assert_eq!(params.len(), 1);
        match params[0].as_value().unwrap() {
            ParamPlan::Encoded {
                ty: TypeRef::String,
                codec,
                shape: native::BufferShape::Slice,
                receive: (),
            } => {
                assert_eq!(codec.root(), &CodecNode::String);
            }
            other => panic!("expected encoded string callback param, got {other:?}"),
        }
    }

    #[test]
    fn callback_method_with_closure_param_lowers_to_outgoing_closure() {
        let mut callback = listener_callback();
        let mut handle = method("on_event", Receiver::Shared);
        handle.parameters = vec![value_param("callback", closure())];
        callback.methods.push(handle);

        let bindings = lower_callback::<Native>(callback);
        let methods = first_callback(&bindings).protocol().vtable().methods();
        let params = methods[0].callable().params();

        assert_eq!(params.len(), 1);
        let outgoing = params[0]
            .as_closure()
            .expect("expected outgoing closure param");
        assert_eq!(outgoing.form(), crate::ClosureForm::Fn);
        assert_eq!(outgoing.presence(), HandlePresence::Required);
        assert_eq!(outgoing.invoke().params().len(), 1);
        match outgoing.invoke().params()[0].as_value().unwrap() {
            ParamPlan::Direct {
                ty: TypeRef::Primitive(crate::Primitive::U32),
                receive: Receive::ByValue,
            } => {}
            other => panic!("expected u32 direct param on invoke, got {other:?}"),
        }
        assert!(matches!(
            outgoing.invoke().returns().plan(),
            ReturnPlan::Void
        ));
    }

    #[test]
    fn callback_method_with_nullable_closure_param_lowers_to_nullable_outgoing_closure() {
        let mut callback = listener_callback();
        let mut handle = method("on_event", Receiver::Shared);
        handle.parameters = vec![value_param("callback", nullable(closure()))];
        callback.methods.push(handle);

        let bindings = lower_callback::<Native>(callback);
        let methods = first_callback(&bindings).protocol().vtable().methods();
        let params = methods[0].callable().params();

        let outgoing = params[0]
            .as_closure()
            .expect("expected nullable outgoing closure param");
        assert_eq!(outgoing.presence(), HandlePresence::Nullable);
        assert_eq!(outgoing.form(), crate::ClosureForm::Fn);
        assert!(matches!(
            outgoing.registration().shape(),
            native::ClosureRegistration::InvokeContextRelease
        ));
        assert!(matches!(
            outgoing.invoke().returns().plan(),
            ReturnPlan::Void
        ));
    }

    #[test]
    fn wasm32_callback_method_with_closure_param_lowers_to_outgoing_closure() {
        let mut callback = listener_callback();
        let mut handle = method("on_event", Receiver::Shared);
        handle.parameters = vec![value_param("callback", closure())];
        callback.methods.push(handle);

        let bindings = lower_callback::<Wasm32>(callback);
        let methods = first_callback(&bindings).protocol().methods();
        let params = methods[0].callable().params();

        assert_eq!(params.len(), 1);
        let outgoing = params[0]
            .as_closure()
            .expect("expected outgoing closure param");
        assert_eq!(outgoing.form(), crate::ClosureForm::Fn);
        assert_eq!(outgoing.presence(), HandlePresence::Required);
        assert_eq!(
            outgoing.registration().shape().call().name().as_str(),
            "boltffi_closure_1____closure__u32_call"
        );
        assert_eq!(
            outgoing.registration().shape().free().name().as_str(),
            "boltffi_closure_1____closure__u32_free"
        );
        let symbol_names = bindings
            .symbols()
            .symbols()
            .iter()
            .map(|symbol| symbol.name().as_str())
            .collect::<Vec<_>>();
        assert!(symbol_names.contains(&"boltffi_closure_1____closure__u32_call"));
        assert!(symbol_names.contains(&"boltffi_closure_1____closure__u32_free"));

        let invoke = outgoing.invoke();
        assert_eq!(invoke.params().len(), 1);
        match invoke.params()[0].as_value().unwrap() {
            ParamPlan::Direct {
                ty: TypeRef::Primitive(crate::Primitive::U32),
                receive: Receive::ByValue,
            } => {}
            other => panic!("expected u32 direct param on wasm closure invoke, got {other:?}"),
        }
        assert!(matches!(invoke.returns().plan(), ReturnPlan::Void));
        assert!(matches!(invoke.error(), ErrorDecl::None(_)));
    }

    #[test]
    fn wasm32_callback_method_returning_closure_lowers_to_closure_via_out_pointer() {
        let mut callback = listener_callback();
        let mut handler_factory = method("handler", Receiver::Shared);
        handler_factory.returns = ReturnDef::value(closure_returning(ReturnDef::value(
            TypeExpr::Primitive(Primitive::U32),
        )));
        callback.methods.push(handler_factory);

        let bindings = lower_callback::<Wasm32>(callback);
        let methods = first_callback(&bindings).protocol().methods();
        let plan = methods[0].callable().returns().plan();

        // Callback method's return direction is `IntoRust` (foreign-implemented
        // body returns to Rust). The closure was created by foreign code, so
        // the invoke contract is an `ImportedCallable` and the registration
        // uses wasm32's `IncomingClosureRegistration` (import-side metadata).
        let closure_crossing = match plan {
            ReturnPlan::ClosureViaOutPointer(crossing) => crossing,
            other => panic!("expected ClosureViaOutPointer, got {other:?}"),
        };
        assert_eq!(closure_crossing.form(), crate::ClosureForm::Fn);
        assert_eq!(closure_crossing.presence(), HandlePresence::Required);
        let invoke = closure_crossing.invoke();
        assert_eq!(invoke.params().len(), 1);
        match invoke.params()[0].as_value().unwrap() {
            ParamPlan::Direct {
                ty: TypeRef::Primitive(crate::Primitive::U32),
                receive: (),
            } => {}
            other => panic!("expected u32 invoke param with OutOfRust direction, got {other:?}"),
        }
        match invoke.returns().plan() {
            ReturnPlan::DirectViaReturnSlot {
                ty: TypeRef::Primitive(crate::Primitive::U32),
            } => {}
            other => panic!("expected u32 direct invoke return, got {other:?}"),
        }

        // Imports (call + free) live on the wasm import table, not the
        // native symbol table, so the contract's symbol-table check covers
        // them at link time rather than here.
        let import_module = closure_crossing
            .registration()
            .shape()
            .call()
            .module()
            .as_str();
        assert_eq!(import_module, "env");
    }

    #[test]
    fn callback_method_returning_nullable_closure_lowers_to_nullable_crossing() {
        let mut callback = listener_callback();
        let mut handler_factory = method("handler", Receiver::Shared);
        handler_factory.returns = ReturnDef::value(nullable(closure_returning(ReturnDef::value(
            TypeExpr::Primitive(Primitive::U32),
        ))));
        callback.methods.push(handler_factory);

        let bindings = lower_callback::<Wasm32>(callback);
        let methods = first_callback(&bindings).protocol().methods();

        match methods[0].callable().returns().plan() {
            ReturnPlan::ClosureViaOutPointer(closure) => {
                assert_eq!(closure.presence(), HandlePresence::Nullable);
                assert_eq!(closure.form(), crate::ClosureForm::Fn);
                assert_eq!(
                    closure.registration().shape().call().module().as_str(),
                    "env"
                );
            }
            other => panic!("expected nullable closure return, got {other:?}"),
        }
    }

    #[test]
    fn native_callback_method_returning_closure_lowers_to_closure_via_out_pointer() {
        let mut callback = listener_callback();
        let mut handler_factory = method("handler", Receiver::Shared);
        handler_factory.returns = ReturnDef::value(closure_returning(ReturnDef::value(
            TypeExpr::Primitive(Primitive::U32),
        )));
        callback.methods.push(handler_factory);

        let bindings = lower_callback::<Native>(callback);
        let methods = first_callback(&bindings).protocol().vtable().methods();
        let plan = methods[0].callable().returns().plan();

        let closure_crossing = match plan {
            ReturnPlan::ClosureViaOutPointer(crossing) => crossing,
            other => panic!("expected ClosureViaOutPointer, got {other:?}"),
        };
        // Callback method's return direction is IntoRust: foreign returns to
        // Rust, and the closure body lives on the foreign side. The return
        // plan forces out-pointer carriage; parameter closure handling cannot
        // accidentally cover this case.
        assert_eq!(closure_crossing.form(), crate::ClosureForm::Fn);
        assert_eq!(closure_crossing.presence(), HandlePresence::Required);
        assert_eq!(
            closure_crossing.registration().shape(),
            &native::ClosureRegistration::InvokeContextRelease
        );
        let invoke = closure_crossing.invoke();
        assert_eq!(invoke.params().len(), 1);
        match invoke.params()[0].as_value().unwrap() {
            ParamPlan::Direct {
                ty: TypeRef::Primitive(crate::Primitive::U32),
                receive: (),
            } => {}
            other => panic!("expected u32 invoke param with OutOfRust direction, got {other:?}"),
        }
        match invoke.returns().plan() {
            ReturnPlan::DirectViaReturnSlot {
                ty: TypeRef::Primitive(crate::Primitive::U32),
            } => {}
            other => panic!("expected u32 direct invoke return, got {other:?}"),
        }
    }

    #[test]
    fn callback_method_returning_string_uses_write_codec() {
        let mut callback = listener_callback();
        let mut describe = method("describe", Receiver::Shared);
        describe.returns = ReturnDef::value(TypeExpr::String);
        callback.methods.push(describe);

        let bindings = lower_callback::<Native>(callback);
        let methods = first_callback(&bindings).protocol().vtable().methods();

        match methods[0].callable().returns().plan() {
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::String,
                codec,
                shape: native::BufferShape::Buffer,
            } => {
                assert_eq!(codec.value(), &ValueRef::self_value());
                assert_eq!(codec.root(), &CodecNode::String);
            }
            other => panic!("expected encoded string return, got {other:?}"),
        }
    }

    #[test]
    fn box_dyn_callback_param_lowers_to_required_callback_handle() {
        let bindings = lower_record_with_listener_param::<Native>(boxed_listener())
            .expect("contract should lower");

        match record_first_method_lower_plan(&bindings) {
            ParamPlan::Handle {
                target: HandleTarget::Callback(_),
                carrier: native::HandleCarrier::CallbackHandle,
                receive: Receive::ByValue,
                presence: HandlePresence::Required,
            } => {}
            other => panic!("expected required boxed callback handle, got {other:?}"),
        }
    }

    #[test]
    fn impl_trait_callback_param_lowers_to_required_callback_handle() {
        let bindings = lower_record_with_listener_param::<Native>(impl_listener())
            .expect("impl Trait callback should lower");

        match record_first_method_lower_plan(&bindings) {
            ParamPlan::Handle {
                target: HandleTarget::Callback(_),
                carrier: native::HandleCarrier::CallbackHandle,
                receive: Receive::ByValue,
                presence: HandlePresence::Required,
            } => {}
            other => panic!("expected required impl-trait callback handle, got {other:?}"),
        }
    }

    #[test]
    fn arc_dyn_callback_param_lowers_to_required_callback_handle() {
        let bindings = lower_record_with_listener_param::<Native>(arc_listener())
            .expect("Arc<dyn> callback should lower");

        match record_first_method_lower_plan(&bindings) {
            ParamPlan::Handle {
                target: HandleTarget::Callback(_),
                carrier: native::HandleCarrier::CallbackHandle,
                receive: Receive::ByValue,
                presence: HandlePresence::Required,
            } => {}
            other => panic!("expected required arc callback handle, got {other:?}"),
        }
    }

    #[test]
    fn option_box_dyn_callback_param_lowers_to_nullable_callback_handle() {
        let bindings = lower_record_with_listener_param::<Native>(nullable(boxed_listener()))
            .expect("Option<Box<dyn Listener>> param must lower as a nullable callback handle");

        match record_first_method_lower_plan(&bindings) {
            ParamPlan::Handle {
                target: HandleTarget::Callback(_),
                carrier: native::HandleCarrier::CallbackHandle,
                receive: Receive::ByValue,
                presence: HandlePresence::Nullable,
            } => {}
            other => panic!("expected nullable boxed callback handle, got {other:?}"),
        }
    }

    #[test]
    fn option_arc_dyn_callback_param_lowers_to_nullable_callback_handle() {
        let bindings = lower_record_with_listener_param::<Native>(nullable(arc_listener()))
            .expect("Option<Arc<dyn Listener>> should lower");

        match record_first_method_lower_plan(&bindings) {
            ParamPlan::Handle {
                target: HandleTarget::Callback(_),
                carrier: native::HandleCarrier::CallbackHandle,
                receive: Receive::ByValue,
                presence: HandlePresence::Nullable,
            } => {}
            other => panic!("expected nullable arc callback handle, got {other:?}"),
        }
    }

    #[test]
    fn option_impl_trait_callback_param_lowers_to_nullable_callback_handle() {
        let bindings = lower_record_with_listener_param::<Native>(nullable(impl_listener()))
            .expect("Option<impl Listener> should lower");

        match record_first_method_lower_plan(&bindings) {
            ParamPlan::Handle {
                target: HandleTarget::Callback(_),
                carrier: native::HandleCarrier::CallbackHandle,
                receive: Receive::ByValue,
                presence: HandlePresence::Nullable,
            } => {}
            other => panic!("expected nullable impl-trait callback handle, got {other:?}"),
        }
    }

    #[test]
    fn borrowed_impl_trait_callback_param_is_rejected() {
        let error = lower_record_with_listener_param_passing::<Native>(
            impl_listener(),
            ParameterPassing::Ref,
        )
        .expect_err("borrowed impl Trait callback param must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::BorrowedCallbackParameter)
        ));
    }

    #[test]
    fn borrowed_dyn_callback_object_param_is_rejected_without_panicking() {
        let error = lower_record_with_listener_param_passing::<Native>(
            borrowed_listener_object(),
            ParameterPassing::Ref,
        )
        .expect_err("borrowed dyn Listener callback param must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::BorrowedCallbackParameter)
        ));
    }

    #[test]
    fn mutable_borrowed_box_dyn_callback_param_is_rejected() {
        let error = lower_record_with_listener_param_passing::<Native>(
            boxed_listener(),
            ParameterPassing::RefMut,
        )
        .expect_err("borrowed Box<dyn Listener> callback param must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::BorrowedCallbackParameter)
        ));
    }

    #[test]
    fn nullable_callback_param_uses_same_carrier_as_required() {
        let required = lower_record_with_listener_param::<Native>(boxed_listener())
            .expect("required should lower");
        let nullable = lower_record_with_listener_param::<Native>(nullable(boxed_listener()))
            .expect("nullable should lower");

        let required_carrier = match record_first_method_lower_plan(&required) {
            ParamPlan::Handle { carrier, .. } => *carrier,
            other => panic!("expected handle plan, got {other:?}"),
        };
        let nullable_carrier = match record_first_method_lower_plan(&nullable) {
            ParamPlan::Handle { carrier, .. } => *carrier,
            other => panic!("expected handle plan, got {other:?}"),
        };
        assert_eq!(
            required_carrier, nullable_carrier,
            "nullable callback param must cross with the same carrier as required; \
             nullability is presence-only, not carrier-divergent"
        );
    }

    #[test]
    fn wasm32_nullable_callback_param_uses_u32_carrier() {
        let bindings = lower_record_with_listener_param::<Wasm32>(nullable(boxed_listener()))
            .expect("wasm32 Option<Box<dyn Listener>> should lower");

        match record_first_method_lower_plan(&bindings) {
            ParamPlan::Handle {
                target: HandleTarget::Callback(_),
                carrier: wasm32::HandleCarrier::U32,
                receive: Receive::ByValue,
                presence: HandlePresence::Nullable,
            } => {}
            other => panic!("expected wasm32 nullable callback handle, got {other:?}"),
        }
    }

    #[test]
    fn class_method_returning_callback_lowers_to_required_lift_handle() {
        let bindings = lower_class_returning_listener::<Native>(boxed_listener())
            .expect("contract should lower");

        match class_first_method_lift_plan(&bindings) {
            ReturnPlan::HandleViaReturnSlot {
                target: HandleTarget::Callback(_),
                carrier: native::HandleCarrier::CallbackHandle,
                presence: HandlePresence::Required,
            } => {}
            other => panic!("expected required callback handle return, got {other:?}"),
        }
    }

    #[test]
    fn class_method_returning_arc_callback_lowers_to_required_lift_handle() {
        let bindings = lower_class_returning_listener::<Native>(arc_listener())
            .expect("Arc<dyn Listener> return should lower");

        match class_first_method_lift_plan(&bindings) {
            ReturnPlan::HandleViaReturnSlot {
                target: HandleTarget::Callback(_),
                carrier: native::HandleCarrier::CallbackHandle,
                presence: HandlePresence::Required,
            } => {}
            other => panic!("expected required arc callback handle return, got {other:?}"),
        }
    }

    #[test]
    fn class_method_returning_optional_arc_callback_lowers_to_nullable_lift_handle() {
        let bindings = lower_class_returning_listener::<Native>(nullable(arc_listener()))
            .expect("Option<Arc<dyn Listener>> return should lower");

        match class_first_method_lift_plan(&bindings) {
            ReturnPlan::HandleViaReturnSlot {
                target: HandleTarget::Callback(_),
                carrier: native::HandleCarrier::CallbackHandle,
                presence: HandlePresence::Nullable,
            } => {}
            other => panic!("expected nullable arc callback handle return, got {other:?}"),
        }
    }

    #[test]
    fn class_method_returning_optional_callback_lowers_to_nullable_lift_handle() {
        let bindings = lower_class_returning_listener::<Native>(nullable(boxed_listener()))
            .expect("Option<Box<dyn Listener>> return should lower");

        match class_first_method_lift_plan(&bindings) {
            ReturnPlan::HandleViaReturnSlot {
                target: HandleTarget::Callback(_),
                carrier: native::HandleCarrier::CallbackHandle,
                presence: HandlePresence::Nullable,
            } => {}
            other => panic!("expected nullable callback handle return, got {other:?}"),
        }
    }

    #[test]
    fn wasm32_callback_handle_param_uses_u32_carrier() {
        let bindings = lower_record_with_listener_param::<Wasm32>(boxed_listener())
            .expect("contract should lower");

        match record_first_method_lower_plan(&bindings) {
            ParamPlan::Handle {
                target: HandleTarget::Callback(_),
                carrier: wasm32::HandleCarrier::U32,
                receive: Receive::ByValue,
                presence: HandlePresence::Required,
            } => {}
            other => panic!("expected wasm32 callback handle param, got {other:?}"),
        }
    }

    #[test]
    fn callback_method_returning_self_is_rejected() {
        let mut callback = listener_callback();
        let mut clone_self = method("clone_self", Receiver::Shared);
        clone_self.returns = ReturnDef::value(TypeExpr::SelfType);
        callback.methods.push(clone_self);

        let mut contract = package();
        contract.traits.push(callback);
        let error = lower::<Native>(&contract).expect_err("Self return must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::SelfInCallbackTrait)
        ));
    }

    #[test]
    fn callback_method_taking_self_param_is_rejected() {
        let mut callback = listener_callback();
        let mut compare = method("compare", Receiver::Shared);
        compare.parameters = vec![value_param("other", TypeExpr::SelfType)];
        callback.methods.push(compare);

        let mut contract = package();
        contract.traits.push(callback);
        let error = lower::<Native>(&contract).expect_err("Self parameter must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::SelfInCallbackTrait)
        ));
    }

    #[test]
    fn callback_method_returning_vec_of_self_is_rejected() {
        let mut callback = listener_callback();
        let mut clones = method("clones", Receiver::Shared);
        clones.returns = ReturnDef::value(TypeExpr::Vec(Box::new(TypeExpr::SelfType)));
        callback.methods.push(clones);

        let mut contract = package();
        contract.traits.push(callback);
        let error = lower::<Native>(&contract).expect_err("Vec<Self> must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::SelfInCallbackTrait)
        ));
    }

    #[test]
    fn callback_method_named_free_is_rejected_as_slot_collision() {
        let mut callback = listener_callback();
        callback.methods.push(method("free", Receiver::Shared));

        let mut contract = package();
        contract.traits.push(callback);
        let error = lower::<Native>(&contract).expect_err("method named free must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::CallbackMethodSlotCollision)
        ));
    }

    #[test]
    fn callback_method_named_clone_is_rejected_as_slot_collision() {
        let mut callback = listener_callback();
        callback.methods.push(method("clone", Receiver::Shared));

        let mut contract = package();
        contract.traits.push(callback);
        let error = lower::<Native>(&contract).expect_err("method named clone must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::CallbackMethodSlotCollision)
        ));
    }

    #[test]
    fn callback_methods_that_snake_case_to_same_name_are_rejected() {
        let mut callback = listener_callback();
        callback.methods.push(method("onURL", Receiver::Shared));
        callback.methods.push(method("on_url", Receiver::Shared));

        let mut contract = package();
        contract.traits.push(callback);
        let error = lower::<Native>(&contract).expect_err("colliding snake-case names must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::CallbackMethodSlotCollision)
        ));
    }

    #[test]
    fn callback_method_with_no_receiver_is_rejected() {
        let mut callback = listener_callback();
        callback.methods.push(method("greet", Receiver::None));

        let mut contract = package();
        contract.traits.push(callback);
        let error = lower::<Native>(&contract).expect_err("static method must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::InvalidCallbackReceiver)
        ));
    }

    #[test]
    fn callback_method_with_owned_receiver_is_rejected() {
        let mut callback = listener_callback();
        callback.methods.push(method("consume", Receiver::Owned));

        let mut contract = package();
        contract.traits.push(callback);
        let error = lower::<Native>(&contract).expect_err("owned receiver must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::InvalidCallbackReceiver)
        ));
    }

    #[test]
    fn callback_handle_target_carries_exact_callback_id() {
        let mut contract = package();
        contract.traits.push(listener_callback());
        let mut other = TraitDef::new("demo::Observer".into(), name("Observer"));
        other.methods.push(method("on_change", Receiver::Shared));
        contract.traits.push(other);

        let mut record = RecordDef::new("demo::Engine".into(), name("Engine"));
        record.fields = vec![FieldDef::new(
            name("ticks"),
            TypeExpr::Primitive(Primitive::U32),
        )];
        let mut install = method("install", Receiver::Mutable);
        install.parameters = vec![value_param(
            "observer",
            boxed_callback_trait("demo::Observer", "Observer"),
        )];
        record.methods.push(install);
        contract.records.push(record);

        let bindings = lower::<Native>(&contract).expect("contract should lower");
        let observer_id = bindings
            .decls()
            .iter()
            .find_map(|decl| match decl {
                Decl::Callback(callback) if callback.name().as_path_string() == "Observer" => {
                    Some(callback.id())
                }
                _ => None,
            })
            .expect("expected Observer callback");

        let plan = record_first_method_lower_plan(&bindings);
        match plan {
            ParamPlan::Handle {
                target: HandleTarget::Callback(id),
                ..
            } => assert_eq!(id, &observer_id),
            other => panic!("expected callback handle, got {other:?}"),
        }
    }

    #[test]
    fn native_callback_symbol_table_contains_register_and_create_handle() {
        let bindings = lower_callback::<Native>(listener_callback());
        let names: Vec<&str> = bindings
            .symbols()
            .symbols()
            .iter()
            .map(|symbol| symbol.name().as_str())
            .collect();
        assert!(names.contains(&"boltffi_register_callback_demo_listener"));
        assert!(names.contains(&"boltffi_create_callback_demo_listener"));
    }

    #[test]
    fn wasm32_callback_symbol_table_contains_only_create_handle() {
        let bindings = lower_callback::<Wasm32>(listener_callback());
        let names: Vec<&str> = bindings
            .symbols()
            .symbols()
            .iter()
            .map(|symbol| symbol.name().as_str())
            .collect();
        assert!(names.contains(&"boltffi_create_callback_demo_listener"));
        assert!(!names.contains(&"boltffi_register_callback_demo_listener"));
    }

    #[test]
    fn multiple_callback_methods_get_sequential_ids_in_source_order() {
        let mut callback = listener_callback();
        callback.methods.push(method("on_start", Receiver::Shared));
        callback.methods.push(method("on_tick", Receiver::Shared));
        callback.methods.push(method("on_stop", Receiver::Shared));

        let bindings = lower_callback::<Native>(callback);
        let methods = first_callback(&bindings).protocol().vtable().methods();

        assert_eq!(methods.len(), 3);
        assert_eq!(methods[0].id().raw(), 0);
        assert_eq!(methods[1].id().raw(), 1);
        assert_eq!(methods[2].id().raw(), 2);
        assert_eq!(methods[0].target().as_str(), "on_start");
        assert_eq!(methods[1].target().as_str(), "on_tick");
        assert_eq!(methods[2].target().as_str(), "on_stop");
    }

    #[test]
    fn callback_doc_and_deprecation_propagate_to_decl_meta() {
        let mut callback = listener_callback();
        callback.doc = Some(SourceDocComment::new("event listener"));
        callback.deprecated = Some(SourceDeprecationInfo {
            note: Some("use Observer instead".to_owned()),
            since: Some("0.5".to_owned()),
        });

        let bindings = lower_callback::<Native>(callback);
        let meta = first_callback(&bindings).meta();

        assert_eq!(meta.doc().map(|d| d.as_str()), Some("event listener"));
        assert_eq!(
            meta.deprecated().and_then(|d| d.message()),
            Some("use Observer instead")
        );
    }

    #[test]
    fn callback_method_doc_and_deprecation_propagate() {
        let mut callback = listener_callback();
        let mut on_event = method("on_event", Receiver::Shared);
        on_event.doc = Some(SourceDocComment::new("fires on event"));
        on_event.deprecated = Some(SourceDeprecationInfo {
            note: Some("use on_event_v2".to_owned()),
            since: None,
        });
        callback.methods.push(on_event);

        let bindings = lower_callback::<Native>(callback);
        let methods = first_callback(&bindings).protocol().vtable().methods();
        let meta = methods[0].meta();

        assert_eq!(meta.doc().map(|d| d.as_str()), Some("fires on event"));
        assert_eq!(
            meta.deprecated().and_then(|d| d.message()),
            Some("use on_event_v2")
        );
    }

    #[test]
    fn class_method_taking_optional_callback_lowers_to_nullable_callback_handle() {
        let mut contract = package();
        contract.traits.push(listener_callback());
        let mut class = ClassDef::new("demo::Engine".into(), name("Engine"));
        let mut maybe_listener = method("maybe_listener", Receiver::Shared);
        maybe_listener.parameters = vec![value_param("listener", nullable(boxed_listener()))];
        class.methods.push(maybe_listener);
        contract.classes.push(class);

        let bindings = lower::<Native>(&contract)
            .expect("Option<Box<dyn Listener>> class param must lower as nullable callback handle");
        let methods = bindings
            .decls()
            .iter()
            .find_map(|decl| match decl {
                Decl::Class(class) => Some(class.methods()),
                _ => None,
            })
            .expect("expected class");

        match methods[0].callable().params()[0].as_value().unwrap() {
            ParamPlan::Handle {
                target: HandleTarget::Callback(_),
                carrier: native::HandleCarrier::CallbackHandle,
                receive: Receive::ByValue,
                presence: HandlePresence::Nullable,
            } => {}
            other => panic!("expected nullable callback handle param on class, got {other:?}"),
        }
    }

    #[test]
    fn result_unit_ok_emits_void_lift_with_encoded_error() {
        let mut callback = listener_callback();
        let mut try_handle = method("try_handle", Receiver::Shared);
        try_handle.returns = ReturnDef::value(TypeExpr::result(TypeExpr::Unit, TypeExpr::String));
        callback.methods.push(try_handle);

        let bindings = lower_callback::<Native>(callback);
        let methods = first_callback(&bindings).protocol().vtable().methods();
        let callable = methods[0].callable();

        assert!(
            matches!(callable.returns().plan(), ReturnPlan::Void),
            "Result<(), E> must emit Void on the success channel, got {:?}",
            callable.returns().plan()
        );
        match callable.error() {
            ErrorDecl::EncodedViaReturnSlot {
                ty: TypeRef::String,
                ..
            } => {}
            other => panic!("expected encoded String error channel, got {other:?}"),
        }
    }

    #[test]
    fn bare_unit_return_is_rejected_in_favor_of_void() {
        let mut callback = listener_callback();
        let mut bare_unit = method("bare_unit", Receiver::Shared);
        bare_unit.returns = ReturnDef::value(TypeExpr::Unit);
        callback.methods.push(bare_unit);

        let mut contract = package();
        contract.traits.push(callback);
        let error = lower::<Native>(&contract)
            .expect_err("ReturnDef::Value(Unit) is not canonical; use ReturnDef::Void");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::UnitInValuePosition)
        ));
    }

    #[test]
    fn wasm_callback_method_import_snake_cases_camel_case_method_name() {
        let mut callback = listener_callback();
        callback.methods.push(method("onURL", Receiver::Shared));

        let bindings = lower_callback::<Wasm32>(callback);
        let methods = first_callback(&bindings).protocol().methods();

        assert_eq!(methods.len(), 1);
        assert_eq!(
            methods[0].target().name().as_str(),
            "__boltffi_callback_method_demo_listener_on_url"
        );
    }

    #[test]
    fn wasm_callback_method_import_snake_cases_acronym_method_name() {
        let mut callback = listener_callback();
        callback
            .methods
            .push(method("handleHTTPRequest", Receiver::Shared));

        let bindings = lower_callback::<Wasm32>(callback);
        let methods = first_callback(&bindings).protocol().methods();

        assert_eq!(
            methods[0].target().name().as_str(),
            "__boltffi_callback_method_demo_listener_handle_http_request"
        );
    }

    #[test]
    fn native_vtable_slot_matches_wasm_import_suffix_for_camel_case_method() {
        let mut native_cb = listener_callback();
        native_cb.methods.push(method("onURL", Receiver::Shared));
        let native_bindings = lower_callback::<Native>(native_cb);
        let native_slot = first_callback(&native_bindings)
            .protocol()
            .vtable()
            .methods()[0]
            .target()
            .as_str()
            .to_owned();

        let mut wasm_cb = listener_callback();
        wasm_cb.methods.push(method("onURL", Receiver::Shared));
        let wasm_bindings = lower_callback::<Wasm32>(wasm_cb);
        let wasm_import = first_callback(&wasm_bindings).protocol().methods()[0]
            .target()
            .name()
            .as_str()
            .to_owned();
        let wasm_suffix = wasm_import
            .strip_prefix("__boltffi_callback_method_demo_listener_")
            .expect("wasm import must use the documented prefix");

        assert_eq!(
            native_slot, wasm_suffix,
            "native vtable slot and wasm import suffix must be byte-equal so the same source \
             method dispatches to the same identifier on every surface"
        );
    }

    #[test]
    fn acronym_callback_name_lowers_to_snake_cased_symbols() {
        let mut callback = TraitDef::new("demo::HTTPListener".into(), name("HTTPListener"));
        callback
            .methods
            .push(method("on_request", Receiver::Shared));

        let bindings = lower_callback::<Native>(callback);
        let cb = first_callback(&bindings);

        assert_eq!(
            cb.protocol().register().name().as_str(),
            "boltffi_register_callback_demo_http_listener"
        );
        let methods = cb.protocol().vtable().methods();
        assert_eq!(methods[0].target().as_str(), "on_request");

        let wasm_bindings = lower_callback::<Wasm32>(TraitDef {
            id: "demo::HTTPListener".into(),
            name: name("HTTPListener").into(),
            methods: vec![method("on_request", Receiver::Shared)],
            user_attrs: Vec::new(),
            doc: None,
            deprecated: None,
            source: boltffi_ast::Source::exported(),
            source_span: None,
        });
        let wasm_cb = first_callback(&wasm_bindings);
        assert_eq!(
            wasm_cb.protocol().methods()[0].target().name().as_str(),
            "__boltffi_callback_method_demo_http_listener_on_request"
        );
    }

    #[test]
    fn callback_method_callable_is_synchronous_with_no_error_channel() {
        let mut callback = listener_callback();
        callback.methods.push(method("on_event", Receiver::Shared));

        let bindings = lower_callback::<Native>(callback);
        let methods = first_callback(&bindings).protocol().vtable().methods();
        let callable = methods[0].callable();

        assert!(matches!(
            callable.execution(),
            ExecutionDecl::Synchronous(_)
        ));
        assert!(matches!(callable.error(), ErrorDecl::None(_)));
    }

    #[test]
    fn native_async_callback_method_lowers_to_callback_completion_protocol() {
        let mut callback = listener_callback();
        let mut on_event = method("on_event", Receiver::Shared);
        on_event.execution = boltffi_ast::ExecutionKind::Async;
        on_event.parameters = vec![value_param("value", TypeExpr::Primitive(Primitive::I32))];
        on_event.returns = ReturnDef::value(TypeExpr::String);
        callback.methods.push(on_event);

        let bindings = lower_callback::<Native>(callback);
        let method = &first_callback(&bindings).protocol().vtable().methods()[0];
        let callable = method.callable();

        assert_eq!(method.target().as_str(), "on_event");
        assert!(matches!(
            callable.execution(),
            ExecutionDecl::Asynchronous(native::AsyncProtocol::CallbackCompletion)
        ));
        assert!(matches!(
            callable.params()[0].as_value().unwrap(),
            ParamPlan::Direct {
                ty: TypeRef::Primitive(crate::Primitive::I32),
                receive: (),
            }
        ));
        match callable.returns().plan() {
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::String,
                codec,
                shape: native::BufferShape::Buffer,
            } => {
                assert_eq!(codec.value(), &ValueRef::self_value());
                assert_eq!(codec.root(), &CodecNode::String);
            }
            other => panic!("expected encoded async callback return, got {other:?}"),
        }
    }

    #[test]
    fn wasm_async_callback_method_uses_start_import_and_complete_export() {
        let mut callback = listener_callback();
        let mut on_event = method("on_event", Receiver::Shared);
        on_event.execution = boltffi_ast::ExecutionKind::Async;
        on_event.parameters = vec![value_param("value", TypeExpr::Primitive(Primitive::I32))];
        on_event.returns = ReturnDef::value(TypeExpr::String);
        callback.methods.push(on_event);

        let bindings = lower_callback::<Wasm32>(callback);
        let callback = first_callback(&bindings);
        let method = &callback.protocol().methods()[0];

        assert_eq!(method.target().module().as_str(), "env");
        assert_eq!(
            method.target().name().as_str(),
            "__boltffi_callback_async_start_demo_listener_on_event"
        );
        match method.callable().execution() {
            ExecutionDecl::Asynchronous(wasm32::AsyncProtocol::CallbackCompletion { complete }) => {
                assert_eq!(
                    complete.name().as_str(),
                    "boltffi_callback_demo_listener_on_event_complete"
                );
            }
            other => panic!("expected wasm callback completion protocol, got {other:?}"),
        }

        let names: Vec<&str> = bindings
            .symbols()
            .symbols()
            .iter()
            .map(|symbol| symbol.name().as_str())
            .collect();
        assert!(names.contains(&"boltffi_callback_demo_listener_on_event_complete"));
    }

    #[test]
    fn wasm_async_callback_start_import_does_not_collide_with_sync_start_suffix_method() {
        let mut callback = listener_callback();
        let mut foo = method("foo", Receiver::Shared);
        foo.execution = boltffi_ast::ExecutionKind::Async;
        callback.methods.push(foo);
        callback.methods.push(method("foo_start", Receiver::Shared));

        let bindings = lower_callback::<Wasm32>(callback);
        let imports: Vec<&str> = first_callback(&bindings)
            .protocol()
            .methods()
            .iter()
            .map(|method| method.target().name().as_str())
            .collect();

        assert!(imports.contains(&"__boltffi_callback_async_start_demo_listener_foo"));
        assert!(imports.contains(&"__boltffi_callback_method_demo_listener_foo_start"));
    }
}

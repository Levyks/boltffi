use boltffi_ffi_rules::transport::ParamValueStrategy;

use crate::{
    ir::{
        AbiCall, AbiCallbackInvocation, AbiCallbackMethod, AbiContract, AbiEnumField,
        AbiEnumPayload, AbiEnumVariant, AbiParam, AbiRecord, AbiType, CallId, CallbackId,
        CallbackMethodDef, CallbackTraitDef, ConstructorDef, CustomTypeDef, EnumDef, EnumRepr,
        FfiContract, FieldDef, FieldName, FieldReadOp, FunctionId, MethodDef, OffsetExpr, ParamDef,
        ParamRole, PrimitiveType, ReadOp, ReadSeq, RecordDef, RecordId, Transport, WriteOp,
        WriteSeq,
    },
    render::dart::{
        DartBlittableField, DartBlittableLayout, DartCallback, DartCallbackMethod, DartConstructor,
        DartConstructorKind, DartCustomType, DartEnum, DartEnumKind, DartEnumVariant, DartFunction,
        DartFunctionParam, DartLibrary, DartNative, DartNativeCallback, DartNativeCallbackMethod,
        DartNativeFunction, DartNativeFunctionParam, DartNativeType, DartRecord, DartRecordField,
        DartType, NamingConvention,
    },
};

pub struct DartLowerer<'a> {
    ffi: &'a FfiContract,
    abi: &'a AbiContract,
    package_name: &'a str,
}

impl<'a> DartLowerer<'a> {
    pub fn new(ffi: &'a FfiContract, abi: &'a AbiContract, package_name: &'a str) -> Self {
        Self {
            ffi,
            abi,
            package_name,
        }
    }

    fn lower_native_function_param(&self, abi_param: &AbiParam) -> DartNativeFunctionParam {
        let name = match &abi_param.role {
            ParamRole::Input { contract, .. } => match contract.value_strategy() {
                ParamValueStrategy::DirectBuffer(..) | ParamValueStrategy::WireEncoded(..) => {
                    format!(
                        "{}Ptr",
                        NamingConvention::param_name(abi_param.name.as_str())
                    )
                }
                _ => NamingConvention::param_name(abi_param.name.as_str()),
            },
            ParamRole::OutDirect => String::from("_p$outPtr"),
            ParamRole::OutLen { .. } => String::from("_p$outLen"),
            _ => NamingConvention::param_name(abi_param.name.as_str()),
        };

        DartNativeFunctionParam {
            name,
            native_type: DartNativeType::from_abi_param(abi_param),
        }
    }

    fn lower_native_function(&self, abi_call: &AbiCall) -> DartNativeFunction {
        let symbol = abi_call.symbol.to_string();

        let params = abi_call
            .params
            .iter()
            .map(|p| self.lower_native_function_param(p))
            .collect();

        let is_not_leaf = abi_call.params.iter().any(|p| {
            matches!(
                p.abi_type,
                AbiType::InlineCallbackFn { .. } | AbiType::CallbackHandle
            )
        });

        DartNativeFunction {
            symbol,
            params,
            return_type: DartNativeType::from_return_shape_and_error_transport(
                &abi_call.returns,
                &abi_call.error,
            ),
            is_leaf: !is_not_leaf,
        }
    }

    pub fn abi_call_for_function(&self, function: &FunctionId) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|c| match &c.id {
                CallId::Function(id) => id == function,
                _ => false,
            })
            .unwrap()
    }

    pub fn abi_call_for_call_id(&self, call_id: &CallId) -> &AbiCall {
        self.abi.calls.iter().find(|c| &c.id == call_id).unwrap()
    }

    fn abi_record_for(&self, record_id: &RecordId) -> Option<&AbiRecord> {
        self.abi
            .records
            .iter()
            .find(|record| record.id == *record_id)
    }

    fn abi_callback_for(&self, id: &CallbackId) -> Option<&AbiCallbackInvocation> {
        self.abi.callbacks.iter().find(|cb| cb.callback_id == *id)
    }

    fn record_field_read_seq(
        &self,
        abi_record: &AbiRecord,
        field_name: &FieldName,
    ) -> Option<ReadSeq> {
        match abi_record.decode_ops.ops.first() {
            Some(ReadOp::Record { fields, .. }) => fields
                .iter()
                .find(|field| field.name == *field_name)
                .map(|field| field.seq.clone()),
            _ => None,
        }
    }

    fn record_field_write_seq(
        &self,
        abi_record: &AbiRecord,
        field_name: &FieldName,
    ) -> Option<WriteSeq> {
        match abi_record.encode_ops.ops.first() {
            Some(WriteOp::Record { fields, .. }) => fields
                .iter()
                .find(|field| field.name == *field_name)
                .map(|field| field.seq.clone()),
            _ => None,
        }
    }

    fn lower_record_field(&self, field: &FieldDef, abi_record: &AbiRecord) -> DartRecordField {
        let record_field_write_seq = self
            .record_field_write_seq(abi_record, &field.name)
            .unwrap();
        let record_field_read_seq = self.record_field_read_seq(abi_record, &field.name).unwrap();

        DartRecordField {
            name: NamingConvention::property_name(field.name.as_str()),
            offset: 0,
            dart_type: super::emit::type_expr_dart_type(&field.type_expr),
            read_seq: record_field_read_seq,
            write_seq: record_field_write_seq,
        }
    }

    fn lower_record_blittable_field(&self, field: &FieldReadOp) -> DartBlittableField {
        let (primitive, offset) = match field.seq.ops.first() {
            Some(ReadOp::Primitive { primitive, offset }) => (*primitive, offset),
            _ => unreachable!(),
        };
        let offset = match offset {
            OffsetExpr::Base => 0,
            OffsetExpr::BasePlus(offset) => *offset,
            _ => unreachable!(),
        };
        let name = NamingConvention::property_name(field.name.as_str());
        let offset_const_name =
            NamingConvention::priv_const_name(format!("offset_{}", field.name.as_str()).as_str());

        DartBlittableField {
            name,
            offset,
            native_type: DartNativeType::Primitive(primitive),
            primitive,
            offset_const_name,
        }
    }

    fn lower_record_blittable_layout(&self, abi_record: &AbiRecord) -> DartBlittableLayout {
        let fields = match abi_record.decode_ops.ops.first() {
            Some(ReadOp::Record { fields, .. }) => fields
                .iter()
                .map(|f| self.lower_record_blittable_field(f))
                .collect(),
            _ => unreachable!(),
        };

        DartBlittableLayout {
            fields,
            struct_name: NamingConvention::record_struct_name(abi_record.id.as_str()),
            struct_size: abi_record
                .size
                .expect("record.is_blittable <=> size != None"),
        }
    }

    fn lower_param(&self, param: &ParamDef) -> DartFunctionParam {
        DartFunctionParam {
            name: NamingConvention::param_name(param.name.as_str()),
            ty: DartType::from_type_expr(&param.type_expr),
        }
    }

    fn lower_constructor(&self, ctor: &ConstructorDef, id: CallId) -> DartConstructor {
        let abi_call = self.abi_call_for_call_id(&id);

        DartConstructor {
            ffi_name: abi_call.symbol.to_string(),
            params: ctor
                .params()
                .iter()
                .map(|param| self.lower_param(param))
                .collect(),
            kind: match ctor {
                ConstructorDef::Default { .. } => DartConstructorKind::Default,
                ConstructorDef::NamedFactory { name, .. }
                | ConstructorDef::NamedInit { name, .. } => DartConstructorKind::Named {
                    name: NamingConvention::function_name(name.as_str()),
                },
            },
            is_fallible: ctor.is_fallible(),
        }
    }

    fn lower_method(&self, meth: &MethodDef, id: CallId) -> DartFunction {
        let abi_call = self.abi_call_for_call_id(&id);

        DartFunction {
            name: NamingConvention::function_name(meth.id.as_str()),
            ffi_name: abi_call.symbol.to_string(),
            params: meth.params.iter().map(|p| self.lower_param(p)).collect(),
            ret_ty: DartType::from_return_def(&meth.returns),
        }
    }

    fn lower_record(&self, record: &RecordDef) -> DartRecord {
        let name = NamingConvention::class_name(record.id.as_str());

        let abi_record = self.abi_record_for(&record.id).unwrap();

        let fields = record
            .fields
            .iter()
            .map(|f| self.lower_record_field(f, abi_record))
            .collect();

        let blittable_layout = abi_record
            .is_blittable
            .then(|| self.lower_record_blittable_layout(abi_record));

        let constructors = record
            .constructor_calls()
            .map(|(id, ctor_def)| self.lower_constructor(ctor_def, id))
            .collect();

        let methods = record
            .method_calls()
            .map(|(id, meth_def)| self.lower_method(meth_def, id))
            .collect();

        DartRecord {
            name,
            is_error: record.is_error,
            fields,
            blittable_layout,
            constructors,
            methods,
        }
    }

    pub fn lower_custom_type(&self, custom: &CustomTypeDef) -> DartCustomType {
        DartCustomType {
            name: custom.id.to_string(),
            ty: DartType::from_type_expr(&custom.repr),
        }
    }

    fn lower_enum_field(&self, field: &AbiEnumField) -> super::DartEnumField {
        let field_name = super::NamingConvention::property_name(field.name.as_str());

        super::DartEnumField {
            name: field_name,
            dart_type: DartType::from_type_expr(&field.type_expr),
            read_seq: field.decode.clone(),
            write_seq: field.encode.clone(),
        }
    }

    fn lower_enum_variant(&self, variant: &AbiEnumVariant, enum_name: &str) -> DartEnumVariant {
        let variant_name = NamingConvention::property_name(variant.name.as_str());
        let variant_class_name = format!(
            "{}${}",
            enum_name,
            NamingConvention::class_name(variant.name.as_str())
        );

        let fields = match &variant.payload {
            AbiEnumPayload::Unit => Vec::new(),
            AbiEnumPayload::Tuple(abi_enum_fields) | AbiEnumPayload::Struct(abi_enum_fields) => {
                abi_enum_fields
                    .iter()
                    .map(|f| self.lower_enum_field(f))
                    .collect()
            }
        };

        DartEnumVariant {
            name: variant_name,
            class_name: variant_class_name,
            tag: variant.discriminant,
            fields,
        }
    }

    fn lower_enum(&self, enum_def: &EnumDef) -> DartEnum {
        let enum_name = NamingConvention::class_name(enum_def.id.as_str());

        let abi_enum = self
            .abi
            .enums
            .iter()
            .find(|en| en.id == enum_def.id)
            .unwrap();

        let enum_kind = if abi_enum.is_c_style {
            DartEnumKind::Enhanced
        } else {
            DartEnumKind::SealedClass
        };

        let tag_type = match &enum_def.repr {
            EnumRepr::CStyle { tag_type, .. } | EnumRepr::Data { tag_type, .. } => *tag_type,
        };

        let enum_variants = abi_enum
            .variants
            .iter()
            .map(|v| self.lower_enum_variant(v, &enum_name))
            .collect();

        let constructors = enum_def
            .constructor_calls()
            .map(|(id, ctor_def)| self.lower_constructor(ctor_def, id))
            .collect();

        let methods = enum_def
            .method_calls()
            .map(|(id, meth_def)| self.lower_method(meth_def, id))
            .collect();

        DartEnum {
            name: enum_name,
            kind: enum_kind,
            tag_type,
            variants: enum_variants,
            size_expr: abi_enum.encode_ops.size.clone(),
            is_error: enum_def.is_error,
            constructors,
            methods,
        }
    }

    fn lower_native_callback_method(
        &self,
        m: &AbiCallbackMethod,
    ) -> super::DartNativeCallbackMethod {
        assert!(matches!(
            m.params[0].role,
            ParamRole::Input {
                transport: Transport::Callback { .. },
                ..
            }
        ));

        let mut params = vec![DartNativeFunctionParam {
            name: "_p$handle".to_string(),
            native_type: DartNativeType::Primitive(PrimitiveType::U64),
        }];

        params.extend(
            m.params[1..]
                .iter()
                .map(|p| self.lower_native_function_param(p)),
        );

        params.push(DartNativeFunctionParam {
            name: "_p$outStatus".to_string(),
            native_type: DartNativeType::Pointer(Box::new(DartNativeType::Status)),
        });

        let return_type =
            DartNativeType::from_return_shape_and_error_transport(&m.returns, &m.error);

        DartNativeCallbackMethod {
            vtable_field_name: NamingConvention::property_name(m.vtable_field.as_str()),
            params,
            return_type,
        }
    }

    fn lower_callback_method(&self, cb: &CallbackMethodDef) -> DartCallbackMethod {
        let params = cb.params.iter().map(|p| self.lower_param(p)).collect();

        DartCallbackMethod {
            name: NamingConvention::function_name(cb.id.as_str()),
            params,
            ret_ty: DartType::from_return_def(&cb.returns),
        }
    }

    fn lower_callback(&self, cb_def: &CallbackTraitDef) -> DartCallback {
        let abi_cb = self.abi_callback_for(&cb_def.id).unwrap();

        let class_name = NamingConvention::class_name(cb_def.id.as_str());
        let native_decls_class_name = format!("_$$Native${}", class_name);
        let impl_class_name = format!("_I${}", class_name);
        let vtable_struct_name = format!(
            "_I${}",
            NamingConvention::class_name(abi_cb.vtable_type.as_str())
        );
        let handle_map_class_name = format!("{}HandleMap", impl_class_name);
        let handle_map_instance_name = format!("_k${}HandleMap", class_name);
        let create_handle_fn_name = abi_cb.create_fn.to_string();
        let vtable_register_fn_name = abi_cb.register_fn.to_string();

        let methods = cb_def
            .methods
            .iter()
            .map(|cb| self.lower_callback_method(cb))
            .collect();

        let native_methods = abi_cb
            .methods
            .iter()
            .map(|m| self.lower_native_callback_method(m))
            .collect();

        DartCallback {
            class_name,
            impl_class_name,
            handle_map_class_name,
            handle_map_instance_name,
            methods,
            native: DartNativeCallback {
                native_decls_class_name,
                create_handle_fn_name,
                vtable_struct_name,
                vtable_register_fn_name,
                methods: native_methods,
            },
        }
    }

    pub fn library(&self) -> DartLibrary {
        let custom_types = self
            .ffi
            .catalog
            .all_custom_types()
            .map(|t| self.lower_custom_type(t))
            .collect();
        let records = self
            .ffi
            .catalog
            .all_records()
            .map(|r| self.lower_record(r))
            .collect();

        let native_functions = self
            .ffi
            .functions
            .iter()
            .map(|f| {
                let abi_call = self.abi_call_for_function(&f.id);
                self.lower_native_function(abi_call)
            })
            .collect();

        let enums = self
            .ffi
            .catalog
            .all_enums()
            .map(|e| self.lower_enum(e))
            .collect();

        let callbacks = self
            .ffi
            .catalog
            .all_callbacks()
            .map(|cb| self.lower_callback(cb))
            .collect();

        DartLibrary {
            custom_types,
            native: DartNative {
                functions: native_functions,
            },
            records,
            enums,
            callbacks,
        }
    }
}

#[cfg(test)]
mod test {
    use boltffi_ffi_rules::callable::ExecutionKind;

    use crate::{
        ir::{
            self, CallbackId, CallbackKind, CallbackTraitDef, FunctionDef, PackageInfo, ParamDef,
            ParamName, ParamPassing, PrimitiveType, ReturnDef, TypeExpr,
        },
        render::dart::DartEmitter,
    };

    use super::*;

    fn empty_contract() -> FfiContract {
        FfiContract {
            package: PackageInfo {
                name: "test".to_string(),
                version: None,
            },
            functions: vec![],
            catalog: Default::default(),
        }
    }

    fn lower(ffi: &FfiContract) -> DartLibrary {
        let abi = ir::Lowerer::new(ffi).to_abi_contract();

        DartLowerer::new(ffi, &abi, "test").library()
    }

    #[test]
    pub fn native_function_primitive_in() {
        let mut ffi = empty_contract();
        ffi.functions.insert(
            0,
            FunctionDef {
                id: FunctionId::new("echo_u64"),
                params: vec![ParamDef {
                    name: ParamName::new("v"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U64),
                    passing: ParamPassing::Value,
                    doc: None,
                }],
                returns: ReturnDef::Void,
                execution_kind: ExecutionKind::Sync,
                doc: None,
                deprecated: None,
            },
        );

        let library = lower(&ffi);

        assert!(matches!(
            library.native.functions[0].params[0].native_type,
            DartNativeType::Primitive(PrimitiveType::U64)
        ));

        assert_eq!(
            library.native.functions[0].params[0]
                .native_type
                .dart_sub_type(),
            "int".to_string()
        );
    }

    #[test]
    pub fn native_function_primitive_out() {
        let mut ffi = empty_contract();
        ffi.functions.insert(
            0,
            FunctionDef {
                id: FunctionId::new("echo_f32"),
                params: vec![],
                returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::F32)),
                execution_kind: ExecutionKind::Sync,
                doc: None,
                deprecated: None,
            },
        );
        let library = lower(&ffi);

        assert!(matches!(
            library.native.functions[0].return_type,
            DartNativeType::Primitive(PrimitiveType::F32)
        ));
        assert_eq!(
            library.native.functions[0].return_type.dart_sub_type(),
            "double".to_string()
        );
    }

    #[test]
    pub fn native_function_void_out() {
        let mut ffi = empty_contract();
        ffi.functions.insert(
            0,
            FunctionDef {
                id: FunctionId::new("noop"),
                params: vec![],
                returns: ReturnDef::Void,
                execution_kind: ExecutionKind::Sync,
                doc: None,
                deprecated: None,
            },
        );
        let library = lower(&ffi);

        assert!(matches!(
            library.native.functions[0].return_type,
            DartNativeType::Void,
        ));
        assert_eq!(
            library.native.functions[0].return_type.dart_sub_type(),
            "void".to_string()
        );
    }

    #[test]
    pub fn native_function_closure_in() {
        let mut ffi = empty_contract();
        ffi.catalog.insert_callback(CallbackTraitDef {
            id: CallbackId::new("ClosureCb"),
            methods: vec![],
            kind: CallbackKind::Closure,
            doc: None,
        });
        ffi.functions.insert(
            0,
            FunctionDef {
                id: FunctionId::new("function_with_callback"),
                params: vec![ParamDef {
                    name: ParamName::new("cb"),
                    type_expr: TypeExpr::Callback(CallbackId::new("ClosureCb")),
                    passing: ParamPassing::ImplTrait,
                    doc: None,
                }],
                returns: ReturnDef::Void,
                execution_kind: ExecutionKind::Sync,
                doc: None,
                deprecated: None,
            },
        );
        let library = lower(&ffi);

        assert!(
            library.native.functions[0].params[0]
                .native_type
                .native_type()
                .contains("$$ffi.Pointer<$$ffi.NativeFunction<")
        );
        assert!(!library.native.functions[0].is_leaf);
    }

    #[test]
    pub fn blittable_record_produces_dart_ffi_struct() {
        let mut ffi = empty_contract();
        ffi.catalog.insert_record(RecordDef {
            id: RecordId::new("Point"),
            is_repr_c: true,
            is_error: false,
            fields: vec![
                FieldDef {
                    name: FieldName::new("x"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("y"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
            ],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let library = lower(&ffi);

        let output = DartEmitter::emit(&library, "test");

        assert!(library.records[0].blittable_layout.is_some());
        assert!(
            output
                .lib
                .contains("final class _$Point$Struct extends $$ffi.Struct")
        );
    }

    #[test]
    pub fn non_blittable_record_does_not_produce_dart_ffi_struct() {
        let mut ffi = empty_contract();
        ffi.catalog.insert_record(RecordDef {
            id: RecordId::new("Person"),
            is_repr_c: false,
            is_error: false,
            fields: vec![
                FieldDef {
                    name: FieldName::new("age"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U64),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("name"),
                    type_expr: TypeExpr::String,
                    doc: None,
                    default: None,
                },
            ],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let library = lower(&ffi);

        assert!(library.records[0].blittable_layout.is_none());
    }

    #[test]
    pub fn error_record_implements_exception() {
        let mut ffi = empty_contract();
        ffi.catalog.insert_record(RecordDef {
            id: RecordId::new("AppError"),
            is_repr_c: false,
            is_error: true,
            fields: vec![FieldDef {
                name: FieldName::new("details"),
                type_expr: TypeExpr::String,
                doc: None,
                default: None,
            }],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let library = lower(&ffi);

        let output = DartEmitter::emit(&library, "test");

        assert!(library.records[0].is_error);
        assert!(
            output
                .lib
                .contains("final class AppError implements Exception")
        );
    }
}

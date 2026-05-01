use crate::{
    ir::{
        AbiParam, AbiType, BuiltinId, CallbackId, ClassId, CustomTypeId, EnumId, ErrorTransport,
        ParamRole, PrimitiveType, ReadSeq, RecordId, ReturnDef, ReturnShape, SizeExpr, Transport,
        TypeExpr, WriteSeq,
    },
    render::dart::NamingConvention,
};

#[derive(Clone, Debug)]
pub enum DartNativeType {
    Void,
    Primitive(PrimitiveType),
    Function {
        params: Vec<DartNativeType>,
        return_ty: Box<DartNativeType>,
    },
    Pointer(Box<DartNativeType>),
    OwnedBuffer,
    CallbackHandle,
    Status,
    Custom(String),
}

impl DartNativeType {
    pub fn from_abi_type(abi_type: &AbiType) -> Self {
        match abi_type {
            AbiType::Void => DartNativeType::Void,
            AbiType::Bool => DartNativeType::Primitive(PrimitiveType::Bool),
            AbiType::I8 => DartNativeType::Primitive(PrimitiveType::I8),
            AbiType::U8 => DartNativeType::Primitive(PrimitiveType::U8),
            AbiType::I16 => DartNativeType::Primitive(PrimitiveType::I16),
            AbiType::U16 => DartNativeType::Primitive(PrimitiveType::U16),
            AbiType::I32 => DartNativeType::Primitive(PrimitiveType::I32),
            AbiType::U32 => DartNativeType::Primitive(PrimitiveType::U32),
            AbiType::I64 => DartNativeType::Primitive(PrimitiveType::I64),
            AbiType::U64 => DartNativeType::Primitive(PrimitiveType::U64),
            AbiType::ISize => DartNativeType::Primitive(PrimitiveType::ISize),
            AbiType::USize => DartNativeType::Primitive(PrimitiveType::USize),
            AbiType::F32 => DartNativeType::Primitive(PrimitiveType::F32),
            AbiType::F64 => DartNativeType::Primitive(PrimitiveType::F64),
            AbiType::Pointer(primitive) => {
                DartNativeType::Pointer(Box::new(DartNativeType::Primitive(*primitive)))
            }
            AbiType::OwnedBuffer => DartNativeType::OwnedBuffer,
            AbiType::InlineCallbackFn {
                params,
                return_type,
            } => DartNativeType::Function {
                params: params.iter().map(Self::from_abi_type).collect(),
                return_ty: Box::new(Self::from_abi_type(return_type)),
            },
            AbiType::Handle(_) => DartNativeType::Pointer(Box::new(DartNativeType::Void)),
            AbiType::CallbackHandle => DartNativeType::CallbackHandle,
            AbiType::Struct(record_id) => {
                DartNativeType::Custom(NamingConvention::record_struct_name(record_id.as_str()))
            }
        }
    }
    pub fn native_type(&self) -> String {
        match self {
            DartNativeType::Void => "$$ffi.Void".to_string(),
            DartNativeType::Primitive(primitive) => {
                super::emit::primitive_native_type(*primitive).to_string()
            }
            DartNativeType::Function { params, return_ty } => format!(
                "$$ffi.Pointer<$$ffi.NativeFunction<{} Function({})>>",
                return_ty.native_type(),
                params.iter().fold(
                    DartNativeType::Pointer(Box::new(DartNativeType::Void)).native_type(),
                    |acc, ty| acc + ", " + ty.native_type().as_str()
                )
            ),
            DartNativeType::Pointer(inner) => format!("$$ffi.Pointer<{}>", inner.native_type()),
            DartNativeType::OwnedBuffer => "_$$FFIBuf".to_string(),
            DartNativeType::CallbackHandle => "_$$BoltFFICallbackHandle".to_string(),
            DartNativeType::Status => "_$$FFIStatus".to_string(),
            DartNativeType::Custom(ty) => ty.clone(),
        }
    }

    pub fn dart_sub_type(&self) -> String {
        match self {
            DartNativeType::Void => "void".to_string(),
            DartNativeType::Primitive(primitive) => super::emit::primitive_dart_type(*primitive),
            o @ (DartNativeType::Function { .. }
            | DartNativeType::Pointer(..)
            | DartNativeType::OwnedBuffer
            | DartNativeType::CallbackHandle
            | DartNativeType::Status
            | DartNativeType::Custom(..)) => o.native_type(),
        }
    }

    pub fn from_return_shape_and_error_transport(
        return_shape: &ReturnShape,
        error_transport: &ErrorTransport,
    ) -> Self {
        if let Some(Transport::Handle { class_id, .. }) = &return_shape.transport {
            return Self::from_abi_type(&AbiType::Handle(class_id.clone()));
        }

        if matches!(return_shape.transport, Some(Transport::Callback { .. })) {
            return Self::from_abi_type(&AbiType::CallbackHandle);
        }

        if matches!(error_transport, ErrorTransport::Encoded { .. }) {
            return Self::from_abi_type(&AbiType::OwnedBuffer);
        }

        match &return_shape.transport {
            None => {
                if matches!(error_transport, ErrorTransport::StatusCode) {
                    Self::Status
                } else {
                    Self::from_abi_type(&AbiType::Void)
                }
            }
            Some(Transport::Scalar(origin)) => Self::Primitive(origin.primitive()),
            Some(Transport::Composite(layout)) => {
                Self::from_abi_type(&AbiType::Struct(layout.record_id.clone()))
            }
            Some(Transport::Span(_)) => Self::from_abi_type(&AbiType::OwnedBuffer),
            Some(Transport::Handle { .. } | Transport::Callback { .. }) => unreachable!(),
        }
    }

    pub fn from_abi_param(abi_param: &AbiParam) -> Self {
        if let ParamRole::CallbackContext { .. } = &abi_param.role {
            return Self::Pointer(Box::new(Self::Void));
        }

        if let ParamRole::StatusOut = &abi_param.role {
            return Self::Pointer(Box::new(Self::Status));
        }

        let native_type = Self::from_abi_type(&abi_param.abi_type);

        match &abi_param.role {
            ParamRole::OutDirect | ParamRole::OutLen { .. } => Self::Pointer(Box::new(native_type)),
            _ => native_type,
        }
    }

    pub fn field_annot(&self) -> String {
        match self {
            DartNativeType::Void
            | DartNativeType::Function { .. }
            | DartNativeType::Pointer(_)
            | DartNativeType::OwnedBuffer
            | DartNativeType::CallbackHandle
            | DartNativeType::Status
            | DartNativeType::Custom(_) => String::new(),
            primitive @ DartNativeType::Primitive(_) => format!("@{}()", primitive.native_type()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DartNativeFunctionParam {
    pub name: String,
    pub native_type: DartNativeType,
}

#[derive(Debug, Clone)]
pub struct DartNativeFunction {
    pub symbol: String,
    pub params: Vec<DartNativeFunctionParam>,
    pub return_type: DartNativeType,
    pub is_leaf: bool,
}

#[derive(Debug, Clone)]
pub struct DartNative {
    pub functions: Vec<DartNativeFunction>,
}

#[derive(Debug, Clone)]
pub struct DartRecordField {
    pub name: String,
    pub offset: usize,
    pub dart_type: String,
    pub read_seq: ReadSeq,
    pub write_seq: WriteSeq,
}

impl DartRecordField {
    pub fn wire_decode_expr(&self, reader_name: &str) -> String {
        super::emit_reader_read(&self.read_seq, reader_name)
    }

    pub fn wire_encode_expr(&self, writer_name: &str) -> String {
        super::emit_writer_write(&self.write_seq, writer_name, &self.name)
    }

    pub fn wire_encoded_size_expr(&self) -> String {
        super::emit_size_expr(&self.write_seq.size)
    }
}

#[derive(Debug, Clone)]
pub struct DartBlittableLayout {
    pub struct_name: String,
    pub struct_size: usize,
    pub fields: Vec<DartBlittableField>,
}

#[derive(Debug, Clone)]
pub struct DartBlittableField {
    pub name: String,
    pub primitive: PrimitiveType,
    pub native_type: DartNativeType,
    pub offset_const_name: String,
    pub offset: usize,
}

impl DartBlittableField {
    pub fn blittable_decode_expr(&self, bytes_name: &str) -> String {
        super::emit_read_blittable_value(&self.offset_const_name, self.primitive, bytes_name)
    }

    pub fn blittable_encode_expr(&self, bytes_name: &str) -> String {
        super::emit_write_blittable_value(
            &self.offset_const_name,
            self.primitive,
            &self.name,
            bytes_name,
        )
    }
}

#[derive(Debug, Clone)]
pub enum DartType {
    Void,
    Bool,
    Int,
    Double,
    String,
    Option(Box<DartType>),
    Result {
        ok: Box<DartType>,
        err: Box<DartType>,
    },
    Bytes,
    List(Box<DartType>),
    Function {
        params: Vec<DartType>,
        ret_ty: Box<DartType>,
    },
    Class(ClassId),
    Record(RecordId),
    Enum(EnumId),
    Callback(CallbackId),
    Custom(CustomTypeId),
    Builtin(BuiltinId),
}

impl DartType {
    pub fn from_type_expr(type_expr: &TypeExpr) -> Self {
        match type_expr {
            TypeExpr::Void => DartType::Void,
            TypeExpr::Primitive(primitive) => match primitive {
                PrimitiveType::Bool => DartType::Bool,
                PrimitiveType::I8
                | PrimitiveType::U8
                | PrimitiveType::I16
                | PrimitiveType::U16
                | PrimitiveType::I32
                | PrimitiveType::U32
                | PrimitiveType::I64
                | PrimitiveType::U64
                | PrimitiveType::ISize
                | PrimitiveType::USize => DartType::Int,
                PrimitiveType::F32 | PrimitiveType::F64 => DartType::Double,
            },
            TypeExpr::String => DartType::String,
            TypeExpr::Bytes => DartType::Bytes,
            TypeExpr::Vec(inner) => DartType::List(Box::new(Self::from_type_expr(inner))),
            TypeExpr::Option(inner) => DartType::Option(Box::new(Self::from_type_expr(inner))),
            TypeExpr::Result { ok, err } => DartType::Result {
                ok: Box::new(Self::from_type_expr(ok)),
                err: Box::new(Self::from_type_expr(err)),
            },
            TypeExpr::Record(record_id) => DartType::Record(record_id.clone()),
            TypeExpr::Enum(enum_id) => DartType::Enum(enum_id.clone()),
            TypeExpr::Callback(callback_id) => DartType::Callback(callback_id.clone()),
            TypeExpr::Custom(custom_type_id) => DartType::Custom(custom_type_id.clone()),
            TypeExpr::Builtin(builtin_id) => DartType::Builtin(builtin_id.clone()),
            TypeExpr::Handle(class_id) => DartType::Class(class_id.clone()),
        }
    }

    pub fn from_return_def(return_def: &ReturnDef) -> Self {
        match return_def {
            ReturnDef::Void => DartType::Void,
            ReturnDef::Value(ty) => DartType::from_type_expr(ty),
            ReturnDef::Result { ok, err } => DartType::Result {
                ok: Box::new(DartType::from_type_expr(ok)),
                err: Box::new(DartType::from_type_expr(err)),
            },
        }
    }

    pub fn dart_type(&self) -> String {
        match self {
            DartType::Void => "void".to_string(),
            DartType::Bool => "bool".to_string(),
            DartType::Int => "int".to_string(),
            DartType::Double => "double".to_string(),
            DartType::String => "String".to_string(),
            DartType::Option(inner) => format!("{}?", inner.dart_type()),
            DartType::Result { ok, err } => {
                format!("BoltFFIResult<{}, {}>", ok.dart_type(), err.dart_type())
            }
            DartType::Bytes => "$$typed_data.Uint8List".to_string(),
            DartType::List(inner) => format!("List<{}>", inner.dart_type()),
            DartType::Function { params, ret_ty } => format!(
                "{} Function({})",
                ret_ty.dart_type(),
                params
                    .iter()
                    .map(|ty| ty.dart_type())
                    .reduce(|acc, ty| acc + ", " + ty.as_str())
                    .unwrap_or_default()
            ),
            DartType::Class(class_id) => class_id.to_string(),
            DartType::Record(record_id) => record_id.to_string(),
            DartType::Enum(enum_id) => enum_id.to_string(),
            DartType::Callback(callback_id) => callback_id.to_string(),
            DartType::Custom(custom_type_id) => custom_type_id.to_string(),
            DartType::Builtin(builtin_id) => builtin_id.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DartFunctionParam {
    pub name: String,
    pub ty: DartType,
}

#[derive(Debug, Clone)]
pub struct DartFunction {
    pub name: String,
    pub ffi_name: String,
    pub params: Vec<DartFunctionParam>,
    pub ret_ty: DartType,
}

#[derive(Debug, Clone)]
pub struct DartRecord {
    pub name: String,
    pub is_error: bool,
    pub fields: Vec<DartRecordField>,
    pub blittable_layout: Option<DartBlittableLayout>,
    pub constructors: Vec<DartConstructor>,
    pub methods: Vec<DartFunction>,
}

#[derive(Debug, Clone)]
pub enum DartConstructorKind {
    Default,
    Named { name: String },
}

#[derive(Debug, Clone)]
pub struct DartConstructor {
    pub ffi_name: String,
    pub kind: DartConstructorKind,
    pub params: Vec<DartFunctionParam>,
    pub is_fallible: bool,
}

#[derive(Debug, Clone)]
pub struct DartCustomType {
    pub name: String,
    pub ty: DartType,
}

#[derive(Debug, Clone)]
pub struct DartLibrary {
    pub custom_types: Vec<DartCustomType>,
    pub native: DartNative,
    pub records: Vec<DartRecord>,
    pub enums: Vec<DartEnum>,
    pub callbacks: Vec<DartCallback>,
}

#[derive(Debug, Clone, Copy)]
pub enum DartEnumKind {
    Enhanced,
    SealedClass,
}

#[derive(Debug, Clone)]
pub struct DartEnumField {
    pub name: String,
    pub dart_type: DartType,
    pub read_seq: ReadSeq,
    pub write_seq: WriteSeq,
}

impl DartEnumField {
    pub fn wire_decode_expr(&self, reader_name: &str) -> String {
        super::emit_reader_read(&self.read_seq, reader_name)
    }

    pub fn wire_encode_expr(&self, writer_name: &str) -> String {
        super::emit_writer_write(&self.write_seq, writer_name, &self.name)
    }

    pub fn wire_encoded_size_expr(&self) -> String {
        super::emit_size_expr(&self.write_seq.size)
    }
}

#[derive(Debug, Clone)]
pub struct DartEnumVariant {
    pub name: String,
    pub class_name: String,
    pub tag: i128,
    pub fields: Vec<DartEnumField>,
}

#[derive(Debug, Clone)]
pub struct DartEnum {
    pub name: String,
    pub kind: DartEnumKind,
    pub tag_type: PrimitiveType,
    pub variants: Vec<DartEnumVariant>,
    pub size_expr: SizeExpr,
    pub is_error: bool,
    pub constructors: Vec<DartConstructor>,
    pub methods: Vec<DartFunction>,
}

impl DartEnum {
    pub fn tag_reader_read(&self, reader_name: &str) -> String {
        format!(
            "{reader_name}.{}()",
            super::emit::primitive_read_method(self.tag_type)
        )
    }

    pub fn tag_writer_write(&self, variant: &DartEnumVariant, writer_name: &str) -> String {
        format!(
            "{writer_name}.{}({});",
            super::emit::primitive_write_method(self.tag_type),
            variant.tag
        )
    }

    pub fn tag_dart_type(&self) -> String {
        super::emit::primitive_dart_type(self.tag_type)
    }

    pub fn wire_encoded_size_expr(&self) -> String {
        super::emit_size_expr(&self.size_expr)
    }
}

#[derive(Debug, Clone)]
pub struct DartNativeCallbackMethod {
    pub vtable_field_name: String,
    pub params: Vec<DartNativeFunctionParam>,
    pub return_type: DartNativeType,
}

#[derive(Debug, Clone)]
pub struct DartNativeCallback {
    pub native_decls_class_name: String,
    pub create_handle_fn_name: String,
    pub vtable_struct_name: String,
    pub vtable_register_fn_name: String,
    pub methods: Vec<DartNativeCallbackMethod>,
}

#[derive(Debug, Clone)]
pub struct DartCallbackMethod {
    pub name: String,
    pub params: Vec<DartFunctionParam>,
    pub ret_ty: DartType,
}

#[derive(Debug, Clone)]
pub struct DartCallback {
    pub class_name: String,
    pub impl_class_name: String,
    pub handle_map_class_name: String,
    pub handle_map_instance_name: String,
    pub methods: Vec<DartCallbackMethod>,
    pub native: DartNativeCallback,
}

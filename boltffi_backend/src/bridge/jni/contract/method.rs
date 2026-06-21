use crate::{
    bridge::{
        c::{self, Expression, Identifier, TypeFragment},
        jni::{JniSymbolName, JvmClassPath},
    },
    core::{Error, Result},
};

const JNI_BRIDGE: &str = "jni";

/// Native method exported to the JVM by a generated JNI source file.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct NativeMethod {
    c_function: c::Function,
    symbol: JniSymbolName,
    returns: NativeReturn,
    parameters: Vec<NativeParameter>,
}

impl NativeMethod {
    /// Creates a JNI native method from a C function declaration.
    pub fn new(class: &JvmClassPath, function: &c::Function) -> Result<Self> {
        Ok(Self {
            symbol: JniSymbolName::native_method(class, function.name())?,
            returns: NativeReturn::from_c_type(function.returns())?,
            parameters: NativeParameter::from_c_parameters(function.params())?,
            c_function: function.clone(),
        })
    }

    /// Returns the C bridge function this method calls.
    pub fn c_function(&self) -> &c::Function {
        &self.c_function
    }

    /// Returns the JNI exported C symbol.
    pub fn symbol(&self) -> &JniSymbolName {
        &self.symbol
    }

    /// Returns the JNI return type.
    pub fn returns(&self) -> NativeReturn {
        self.returns
    }

    /// Returns parameters after `JNIEnv*` and `jclass`.
    pub fn parameters(&self) -> &[NativeParameter] {
        &self.parameters
    }

    /// Returns whether this method returns no value.
    pub fn returns_void(&self) -> bool {
        matches!(self.returns, NativeReturn::Void)
    }

    /// Returns whether this method needs an explicit `jboolean` cast.
    pub fn returns_boolean(&self) -> bool {
        matches!(self.returns, NativeReturn::Value(JniType::Boolean))
    }

    /// Returns whether this method returns an owned byte buffer.
    pub fn returns_bytes(&self) -> bool {
        matches!(self.returns, NativeReturn::Bytes)
    }

    /// Returns whether this method checks a returned `FfiStatus`.
    pub fn checks_status(&self) -> bool {
        matches!(self.returns, NativeReturn::Status)
    }
}

/// JNI return behavior for one native method.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum NativeReturn {
    /// The C function returns `void`.
    Void,
    /// The C function returns a scalar value directly.
    Value(JniType),
    /// The C function returns an owned BoltFFI byte buffer.
    Bytes,
    /// The C function returns `FfiStatus` and the JNI method returns `void`.
    Status,
}

impl NativeReturn {
    /// Returns the JNI method return type as C syntax.
    pub fn jni_type(self) -> TypeFragment {
        match self {
            Self::Void | Self::Status => TypeFragment::new("void"),
            Self::Value(ty) => ty.as_type_fragment(),
            Self::Bytes => TypeFragment::new("jbyteArray"),
        }
    }

    /// Returns the temporary C result type used inside the JNI body.
    pub fn c_result_type(self) -> Result<TypeFragment> {
        match self {
            Self::Void => Ok(TypeFragment::new("void")),
            Self::Status => TypeFragment::anonymous(&c::Type::Status),
            Self::Value(ty) => Ok(ty.as_type_fragment()),
            Self::Bytes => TypeFragment::anonymous(&c::Type::Buffer),
        }
    }

    fn from_c_type(ty: &c::Type) -> Result<Self> {
        match ty {
            c::Type::Void => Ok(Self::Void),
            c::Type::Status => Ok(Self::Status),
            c::Type::Buffer => Ok(Self::Bytes),
            ty => JniType::from_scalar_c_type(ty).map(Self::Value),
        }
    }
}

/// JNI parameter accepted by one native method.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct NativeParameter {
    kind: NativeParameterKind,
}

impl NativeParameter {
    /// Returns the generated C parameter name.
    pub fn name(&self) -> &Identifier {
        match &self.kind {
            NativeParameterKind::Scalar(parameter) => parameter.name(),
            NativeParameterKind::Bytes(parameter) => parameter.name(),
        }
    }

    /// Returns the JNI parameter type.
    pub fn ty(&self) -> TypeFragment {
        match &self.kind {
            NativeParameterKind::Scalar(parameter) => parameter.ty().as_type_fragment(),
            NativeParameterKind::Bytes(_) => TypeFragment::new("jbyteArray"),
        }
    }

    /// Returns C bridge call arguments produced from this JNI parameter.
    pub fn c_arguments(&self) -> Vec<Expression> {
        match &self.kind {
            NativeParameterKind::Scalar(parameter) => {
                vec![Expression::identifier(parameter.name().clone())]
            }
            NativeParameterKind::Bytes(parameter) => vec![
                Expression::cast(
                    TypeFragment::new("const uint8_t *"),
                    Expression::identifier(parameter.pointer().clone()),
                ),
                Expression::cast(
                    TypeFragment::new("uintptr_t"),
                    Expression::identifier(parameter.length().clone()),
                ),
            ],
        }
    }

    /// Returns byte-array parameter details when this parameter carries bytes.
    pub fn bytes(&self) -> Option<&BytesParameter> {
        match &self.kind {
            NativeParameterKind::Scalar(_) => None,
            NativeParameterKind::Bytes(parameter) => Some(parameter),
        }
    }

    fn from_c_parameters(parameters: &[c::Parameter]) -> Result<Vec<Self>> {
        let mut index = 0;
        std::iter::from_fn(|| {
            (index < parameters.len()).then(|| {
                let parameter = &parameters[index];
                match BytesParameter::from_pair(parameter, parameters.get(index + 1)) {
                    Ok(Some(bytes)) => {
                        index += 2;
                        Ok(Self {
                            kind: NativeParameterKind::Bytes(bytes),
                        })
                    }
                    Ok(None) => {
                        index += 1;
                        ScalarParameter::from_c_parameter(parameter).map(|scalar| Self {
                            kind: NativeParameterKind::Scalar(scalar),
                        })
                    }
                    Err(error) => {
                        index = parameters.len();
                        Err(error)
                    }
                }
            })
        })
        .collect()
    }
}

/// JNI parameter shape selected from one or more C ABI parameters.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum NativeParameterKind {
    /// A scalar JNI parameter passed directly to the C bridge.
    Scalar(ScalarParameter),
    /// A `jbyteArray` expanded to pointer and length C bridge arguments.
    Bytes(BytesParameter),
}

/// Scalar JNI parameter mapped to one scalar C bridge argument.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ScalarParameter {
    name: Identifier,
    ty: JniType,
}

impl ScalarParameter {
    /// Returns the generated C parameter name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Returns the scalar JNI parameter type.
    pub fn ty(&self) -> JniType {
        self.ty
    }

    fn from_c_parameter(parameter: &c::Parameter) -> Result<Self> {
        Ok(Self {
            name: Identifier::escape(parameter.name())?,
            ty: JniType::from_scalar_c_type(parameter.ty())?,
        })
    }
}

/// Byte-array JNI parameter mapped to pointer and length C bridge arguments.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct BytesParameter {
    name: Identifier,
    pointer: Identifier,
    length: Identifier,
}

impl BytesParameter {
    /// Returns the generated JNI byte-array parameter name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Returns the local pointer variable passed to the C bridge.
    pub fn pointer(&self) -> &Identifier {
        &self.pointer
    }

    /// Returns the local length variable passed to the C bridge.
    pub fn length(&self) -> &Identifier {
        &self.length
    }

    fn from_pair(pointer: &c::Parameter, length: Option<&c::Parameter>) -> Result<Option<Self>> {
        let Some(length) = length else {
            return Ok(None);
        };
        if !Self::is_pointer(pointer.ty()) || !Self::is_length(length.ty()) {
            return Ok(None);
        }
        let Some(name) = pointer.name().strip_suffix("_ptr") else {
            return Ok(None);
        };
        if length.name() != format!("{name}_len") {
            return Ok(None);
        }
        Self::new(name).map(Some)
    }

    fn new(name: &str) -> Result<Self> {
        let name = Identifier::escape(name)?;
        Ok(Self {
            pointer: Identifier::parse(format!("__boltffi_{}_ptr", name.as_str()))?,
            length: Identifier::parse(format!("__boltffi_{}_len", name.as_str()))?,
            name,
        })
    }

    fn is_pointer(ty: &c::Type) -> bool {
        matches!(ty, c::Type::ConstPointer(inner) if matches!(inner.as_ref(), c::Type::Uint8))
    }

    fn is_length(ty: &c::Type) -> bool {
        matches!(ty, c::Type::PointerWidth)
    }
}

/// JNI scalar type used in a native method signature.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum JniType {
    /// `jboolean`.
    Boolean,
    /// `jbyte`.
    Byte,
    /// `jshort`.
    Short,
    /// `jint`.
    Int,
    /// `jlong`.
    Long,
    /// `jfloat`.
    Float,
    /// `jdouble`.
    Double,
}

impl JniType {
    /// Returns the JNI type as C syntax.
    pub fn as_type_fragment(self) -> TypeFragment {
        TypeFragment::new(match self {
            Self::Boolean => "jboolean",
            Self::Byte => "jbyte",
            Self::Short => "jshort",
            Self::Int => "jint",
            Self::Long => "jlong",
            Self::Float => "jfloat",
            Self::Double => "jdouble",
        })
    }

    fn from_scalar_c_type(ty: &c::Type) -> Result<Self> {
        match ty {
            c::Type::Bool => Ok(Self::Boolean),
            c::Type::Int8 | c::Type::Uint8 => Ok(Self::Byte),
            c::Type::Int16 | c::Type::Uint16 => Ok(Self::Short),
            c::Type::Int32 | c::Type::Uint32 => Ok(Self::Int),
            c::Type::Int64
            | c::Type::Uint64
            | c::Type::SignedPointerWidth
            | c::Type::PointerWidth
            | c::Type::FutureHandle
            | c::Type::CallbackHandle => Ok(Self::Long),
            c::Type::Float32 => Ok(Self::Float),
            c::Type::Float64 => Ok(Self::Double),
            c::Type::Void
            | c::Type::Status
            | c::Type::Buffer
            | c::Type::String
            | c::Type::Span
            | c::Type::StreamPollResult
            | c::Type::WaitResult
            | c::Type::Named(_)
            | c::Type::ConstPointer(_)
            | c::Type::MutPointer(_)
            | c::Type::FunctionPointer { .. } => Err(Error::UnsupportedBridge {
                bridge: JNI_BRIDGE,
                shape: "non-scalar C ABI function",
            }),
        }
    }
}

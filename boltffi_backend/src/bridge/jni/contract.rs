use boltffi_binding::Native;

use crate::{
    bridge::{
        c::{self, HeaderInclude, Identifier, TypeFragment},
        jni::{JniSymbolName, JvmClassPath},
    },
    core::{
        BridgeCapabilities, BridgeCapability, BridgeContract, Error, FilePath, Result,
        contract::sealed,
    },
};

const JNI_BRIDGE: &str = "jni";

/// Contract produced by the JNI bridge layer.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct JniBridgeContract {
    capabilities: BridgeCapabilities,
    class: JvmClassPath,
    source_path: FilePath,
    c_header: HeaderInclude,
    methods: Vec<NativeMethod>,
}

/// Native method exported to the JVM by a generated JNI source file.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct NativeMethod {
    c_function: c::Function,
    symbol: JniSymbolName,
    returns: NativeReturn,
    parameters: Vec<NativeParameter>,
}

/// JNI return behavior for one native method.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum NativeReturn {
    /// The C function returns `void`.
    Void,
    /// The C function returns a scalar value directly.
    Value(JniType),
    /// The C function returns `FfiStatus` and the JNI method returns `void`.
    Status,
}

/// JNI parameter accepted by one native method.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct NativeParameter {
    name: Identifier,
    ty: JniType,
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

impl JniBridgeContract {
    /// Builds the JNI bridge contract from the C bridge contract.
    pub fn from_c_bridge(
        class: JvmClassPath,
        source_path: FilePath,
        c_bridge: &c::CBridgeContract,
    ) -> Result<Self> {
        Ok(Self {
            capabilities: c_bridge
                .capabilities()
                .clone()
                .stable(BridgeCapability::Jni),
            c_header: HeaderInclude::from_files(&source_path, c_bridge.header_path())?,
            methods: c_bridge
                .functions()
                .iter()
                .map(|function| NativeMethod::new(&class, function))
                .collect::<Result<Vec<_>>>()?,
            class,
            source_path,
        })
    }

    /// Returns the JVM class that owns generated native methods.
    pub fn class(&self) -> &JvmClassPath {
        &self.class
    }

    /// Returns the generated JNI source path.
    pub fn source_path(&self) -> &FilePath {
        &self.source_path
    }

    /// Returns the C header include path used by the JNI source.
    pub fn c_header(&self) -> &HeaderInclude {
        &self.c_header
    }

    /// Returns generated native methods.
    pub fn methods(&self) -> &[NativeMethod] {
        &self.methods
    }
}

impl BridgeContract for JniBridgeContract {
    type Surface = Native;

    fn capabilities(&self) -> &BridgeCapabilities {
        &self.capabilities
    }
}

impl sealed::BridgeContract for JniBridgeContract {}

impl NativeMethod {
    /// Creates a JNI native method from a C function declaration.
    pub fn new(class: &JvmClassPath, function: &c::Function) -> Result<Self> {
        Ok(Self {
            symbol: JniSymbolName::native_method(class, function.name())?,
            returns: NativeReturn::from_c_type(function.returns())?,
            parameters: function
                .params()
                .iter()
                .map(NativeParameter::from_c_parameter)
                .collect::<Result<Vec<_>>>()?,
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

    /// Returns whether this method checks a returned `FfiStatus`.
    pub fn checks_status(&self) -> bool {
        matches!(self.returns, NativeReturn::Status)
    }
}

impl NativeParameter {
    /// Returns the generated C parameter name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Returns the JNI parameter type.
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

impl NativeReturn {
    /// Returns the JNI method return type as C syntax.
    pub fn jni_type(self) -> TypeFragment {
        match self {
            Self::Void | Self::Status => TypeFragment::new("void"),
            Self::Value(ty) => ty.as_type_fragment(),
        }
    }

    /// Returns the temporary C result type used inside the JNI body.
    pub fn c_result_type(self) -> Result<TypeFragment> {
        match self {
            Self::Void => Ok(TypeFragment::new("void")),
            Self::Status => TypeFragment::anonymous(&c::Type::Status),
            Self::Value(ty) => Ok(ty.as_type_fragment()),
        }
    }

    fn from_c_type(ty: &c::Type) -> Result<Self> {
        match ty {
            c::Type::Void => Ok(Self::Void),
            c::Type::Status => Ok(Self::Status),
            ty => JniType::from_scalar_c_type(ty).map(Self::Value),
        }
    }
}

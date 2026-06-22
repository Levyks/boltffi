use crate::{
    bridge::{
        c::{self, Identifier, TypeFragment},
        jni::{JniType, JvmClassPath},
    },
    core::{Error, Result},
};

const JNI_BRIDGE: &str = "jni";

/// JNI registration for one native callback vtable.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackRegistration {
    class: JvmClassPath,
    global_class: Identifier,
    free_method: Identifier,
    clone_method: Identifier,
    load: Identifier,
    unload: Identifier,
    vtable_type: Identifier,
    vtable: Identifier,
    register: Identifier,
    free: Identifier,
    clone: Identifier,
    methods: Vec<CallbackMethod>,
}

/// JNI method dispatch for one callback vtable slot.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackMethod {
    function: Identifier,
    method: Identifier,
    method_id: Identifier,
    signature: String,
    returns: CallbackReturnKind,
    parameters: Vec<CallbackArgument>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CallbackReturnKind {
    Void {
        c_type: TypeFragment,
    },
    Value {
        c_type: TypeFragment,
        jni_type: JniType,
    },
}

/// One C argument forwarded to a JVM callback bridge method.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackArgument {
    name: Identifier,
    c_type: TypeFragment,
    jni_type: JniType,
}

impl CallbackRegistration {
    /// Creates JNI callback registration from one C callback contract.
    pub fn from_c_callback(class: &JvmClassPath, callback: &c::Callback) -> Result<Self> {
        let stem = callback.vtable().name();
        Ok(Self {
            class: class.callback_class(callback.name())?,
            global_class: Identifier::parse(format!("g_{stem}_class"))?,
            free_method: Identifier::parse(format!("g_{stem}_free_method"))?,
            clone_method: Identifier::parse(format!("g_{stem}_clone_method"))?,
            load: Identifier::parse(format!("boltffi_jni_load_{stem}_callbacks"))?,
            unload: Identifier::parse(format!("boltffi_jni_unload_{stem}_callbacks"))?,
            vtable_type: Identifier::parse(stem)?,
            vtable: Identifier::parse(format!("g_{stem}_vtable"))?,
            register: Identifier::parse(callback.register().name())?,
            free: Identifier::parse(format!("{stem}_free"))?,
            clone: Identifier::parse(format!("{stem}_clone"))?,
            methods: callback
                .methods()
                .iter()
                .map(|slot| CallbackMethod::from_slot(stem, slot))
                .collect::<Result<Vec<_>>>()?,
        })
    }

    /// Returns the JVM callback bridge class.
    pub fn class(&self) -> &JvmClassPath {
        &self.class
    }

    /// Returns the global class reference symbol.
    pub fn global_class(&self) -> &Identifier {
        &self.global_class
    }

    /// Returns the static `free(long)` method id symbol.
    pub fn free_method(&self) -> &Identifier {
        &self.free_method
    }

    /// Returns the static `clone(long)` method id symbol.
    pub fn clone_method(&self) -> &Identifier {
        &self.clone_method
    }

    /// Returns the load hook called from `JNI_OnLoad`.
    pub fn load(&self) -> &Identifier {
        &self.load
    }

    /// Returns the unload hook called from `JNI_OnUnload`.
    pub fn unload(&self) -> &Identifier {
        &self.unload
    }

    /// Returns the C callback vtable type.
    pub fn vtable_type(&self) -> &Identifier {
        &self.vtable_type
    }

    /// Returns the static C vtable instance symbol.
    pub fn vtable(&self) -> &Identifier {
        &self.vtable
    }

    /// Returns the C callback registration function.
    pub fn register(&self) -> &Identifier {
        &self.register
    }

    /// Returns the C vtable `free` slot implementation.
    pub fn free(&self) -> &Identifier {
        &self.free
    }

    /// Returns the C vtable `clone` slot implementation.
    pub fn clone_callback(&self) -> &Identifier {
        &self.clone
    }

    /// Returns registered callback method slots.
    pub fn methods(&self) -> &[CallbackMethod] {
        &self.methods
    }
}

impl CallbackMethod {
    /// Returns the generated C vtable method implementation.
    pub fn function(&self) -> &Identifier {
        &self.function
    }

    /// Returns the JVM static method name.
    pub fn method(&self) -> &Identifier {
        &self.method
    }

    /// Returns the cached JNI method id symbol.
    pub fn method_id(&self) -> &Identifier {
        &self.method_id
    }

    /// Returns the JNI method descriptor.
    pub fn signature(&self) -> &str {
        &self.signature
    }

    /// Returns the C return type for the vtable slot implementation.
    pub fn c_return_type(&self) -> &TypeFragment {
        self.returns.c_type()
    }

    /// Returns whether the slot returns no value.
    pub fn returns_void(&self) -> bool {
        matches!(self.returns, CallbackReturnKind::Void { .. })
    }

    /// Returns the `CallStatic*Method` suffix for non-void slots.
    pub fn call_method_suffix(&self) -> Option<&'static str> {
        self.returns.call_method_suffix()
    }

    /// Returns the C value returned when dispatch fails.
    pub fn failure_value(&self) -> Option<&'static str> {
        self.returns.failure_value()
    }

    /// Returns generated C parameters.
    pub fn parameters(&self) -> &[CallbackArgument] {
        &self.parameters
    }

    fn from_slot(stem: &str, slot: &c::CallbackSlot) -> Result<Self> {
        let Some(c::Type::Uint64) = slot.params().first() else {
            return Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "callback vtable slot does not start with a uint64 handle",
            });
        };
        let returns = CallbackReturnKind::from_c_type(slot.returns())?;
        let parameters = slot
            .params()
            .iter()
            .enumerate()
            .map(CallbackArgument::from_c_type)
            .collect::<Result<Vec<_>>>()?;
        let signature = format!(
            "({}){}",
            parameters
                .iter()
                .map(CallbackArgument::jni_signature)
                .collect::<Vec<_>>()
                .join(""),
            returns.jni_signature()
        );
        Ok(Self {
            function: Identifier::parse(format!("{stem}_{}", slot.name()))?,
            method: slot.name().clone(),
            method_id: Identifier::parse(format!("g_{stem}_{}_method", slot.name()))?,
            signature,
            returns,
            parameters,
        })
    }
}

impl CallbackReturnKind {
    fn from_c_type(ty: &c::Type) -> Result<Self> {
        match ty {
            c::Type::Void => Ok(Self::Void {
                c_type: TypeFragment::anonymous(ty)?,
            }),
            ty => Ok(Self::Value {
                c_type: TypeFragment::anonymous(ty)?,
                jni_type: JniType::from_c_type(ty)?,
            }),
        }
    }

    fn c_type(&self) -> &TypeFragment {
        match self {
            Self::Void { c_type } | Self::Value { c_type, .. } => c_type,
        }
    }

    fn jni_signature(&self) -> &'static str {
        match self {
            Self::Void { .. } => "V",
            Self::Value { jni_type, .. } => jni_type.signature(),
        }
    }

    fn call_method_suffix(&self) -> Option<&'static str> {
        match self {
            Self::Void { .. } => None,
            Self::Value { jni_type, .. } => Some(jni_type.call_method_suffix()),
        }
    }

    fn failure_value(&self) -> Option<&'static str> {
        match self {
            Self::Void { .. } => None,
            Self::Value { jni_type, .. } => Some(jni_type.failure_value()),
        }
    }
}

impl CallbackArgument {
    /// Returns the C parameter name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Returns the C parameter type.
    pub fn c_type(&self) -> &TypeFragment {
        &self.c_type
    }

    /// Returns the JNI type used when calling Java.
    pub fn jni_type(&self) -> TypeFragment {
        self.jni_type.as_type_fragment()
    }

    fn jni_signature(&self) -> &'static str {
        self.jni_type.signature()
    }

    fn from_c_type((index, ty): (usize, &c::Type)) -> Result<Self> {
        Ok(Self {
            name: Identifier::parse(match index {
                0 => "handle".to_owned(),
                index => format!("arg{}", index - 1),
            })?,
            c_type: TypeFragment::anonymous(ty)?,
            jni_type: JniType::from_c_type(ty)?,
        })
    }
}

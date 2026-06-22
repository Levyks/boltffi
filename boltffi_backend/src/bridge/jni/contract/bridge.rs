use boltffi_binding::Native;

use crate::{
    bridge::{
        c::{self, HeaderInclude, Identifier},
        jni::JvmClassPath,
    },
    core::{
        BridgeCapabilities, BridgeCapability, BridgeContract, FilePath, Result, contract::sealed,
    },
};

use super::{CallbackRegistration, NativeMethod};

/// Contract produced by the JNI bridge layer.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct JniBridgeContract {
    capabilities: BridgeCapabilities,
    class: JvmClassPath,
    source_path: FilePath,
    c_header: HeaderInclude,
    free_buffer: Identifier,
    callbacks: Vec<CallbackRegistration>,
    methods: Vec<NativeMethod>,
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
            free_buffer: Identifier::parse(c_bridge.support().buffer_free()?.name())?,
            callbacks: c_bridge
                .callbacks()
                .iter()
                .map(|callback| CallbackRegistration::from_c_callback(&class, callback))
                .collect::<Result<Vec<_>>>()?,
            methods: c_bridge
                .functions()
                .iter()
                .map(|function| NativeMethod::new(&class, function, c_bridge.callbacks()))
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

    /// Returns the C support function that releases owned BoltFFI byte buffers.
    pub fn free_buffer(&self) -> &Identifier {
        &self.free_buffer
    }

    /// Returns generated callback registrations.
    pub fn callbacks(&self) -> &[CallbackRegistration] {
        &self.callbacks
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

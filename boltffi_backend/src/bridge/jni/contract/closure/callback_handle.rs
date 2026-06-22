use boltffi_binding::ClosureSignature;

use crate::{
    bridge::{
        c::{self, Identifier, Statement},
        jni::{JniSymbolName, JvmClassPath},
    },
    core::Result,
};

/// JNI handle contract for a Rust-owned closure passed into a JVM callback.
///
/// Callback trait methods can receive inline closure parameters from Rust. The
/// JNI bridge stores the closure function pointer, context pointer, and release
/// hook behind one `jlong` so JVM code can call or release it later.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallbackClosureHandle {
    ty: Identifier,
    new: Identifier,
    ref_: Identifier,
    release: Identifier,
    call_symbol: JniSymbolName,
    release_symbol: JniSymbolName,
    call_field: Statement,
}

impl CallbackClosureHandle {
    /// Creates the JNI handle contract for one closure signature.
    pub fn new(
        class: &JvmClassPath,
        signature: &ClosureSignature,
        call_type: &c::Type,
    ) -> Result<Self> {
        let stem = signature.symbol_part();
        Ok(Self {
            ty: Identifier::parse(format!("BoltFFIJniClosure{stem}"))?,
            new: Identifier::parse(format!("boltffi_jni_{stem}_handle_new"))?,
            ref_: Identifier::parse(format!("boltffi_jni_{stem}_handle_ref"))?,
            release: Identifier::parse(format!("boltffi_jni_{stem}_handle_release"))?,
            call_symbol: JniSymbolName::native_method(
                class,
                &format!("boltffi_callback_closure_{stem}_call"),
            )?,
            release_symbol: JniSymbolName::native_method(
                class,
                &format!("boltffi_callback_closure_{stem}_release"),
            )?,
            call_field: c::TypeFragment::declaration(call_type, "call")?,
        })
    }

    /// Returns the C struct type storing the closure triple.
    pub fn ty(&self) -> &Identifier {
        &self.ty
    }

    /// Returns the C helper that allocates a closure handle.
    pub fn new_function(&self) -> &Identifier {
        &self.new
    }

    /// Returns the C helper that borrows a closure handle.
    pub fn ref_function(&self) -> &Identifier {
        &self.ref_
    }

    /// Returns the C helper that releases a closure handle.
    pub fn release_function(&self) -> &Identifier {
        &self.release
    }

    /// Returns the JNI native method that invokes the closure.
    pub fn call_symbol(&self) -> &JniSymbolName {
        &self.call_symbol
    }

    /// Returns the JNI native method that releases the closure.
    pub fn release_symbol(&self) -> &JniSymbolName {
        &self.release_symbol
    }

    /// Returns the C function-pointer field declaration.
    pub fn call_field(&self) -> &Statement {
        &self.call_field
    }
}

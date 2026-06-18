use boltffi_binding::{
    Bindings, CallbackDecl, CallbackId, ClassDecl, ClassId, ConstantDecl, ConstantId,
    CustomTypeDecl, CustomTypeId, DeclarationRef, EnumDecl, EnumId, FunctionDecl, FunctionId,
    RecordDecl, RecordId, StreamDecl, StreamId, Surface,
};

/// Read-only state shared while one target renders a binding contract.
#[non_exhaustive]
pub struct RenderContext<'bindings, S: Surface> {
    bindings: &'bindings Bindings<S>,
    target: &'static str,
}

impl<'bindings, S: Surface> RenderContext<'bindings, S> {
    /// Creates a render context for a target.
    pub const fn new(bindings: &'bindings Bindings<S>, target: &'static str) -> Self {
        Self { bindings, target }
    }

    /// Returns the binding contract being rendered.
    pub const fn bindings(&self) -> &'bindings Bindings<S> {
        self.bindings
    }

    /// Returns the backend target name.
    pub const fn target(&self) -> &'static str {
        self.target
    }

    /// Returns the record declaration with the given id.
    pub fn record(&self, id: RecordId) -> Option<&'bindings RecordDecl<S>> {
        self.bindings.decls().iter().find_map(|declaration| {
            match DeclarationRef::from(declaration) {
                DeclarationRef::Record(record) if record.id() == id => Some(record),
                DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            }
        })
    }

    /// Returns the enum declaration with the given id.
    pub fn enumeration(&self, id: EnumId) -> Option<&'bindings EnumDecl<S>> {
        self.bindings.decls().iter().find_map(|declaration| {
            match DeclarationRef::from(declaration) {
                DeclarationRef::Enum(enumeration) if enumeration.id() == id => Some(enumeration),
                DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            }
        })
    }

    /// Returns the class declaration with the given id.
    pub fn class(&self, id: ClassId) -> Option<&'bindings ClassDecl<S>> {
        self.bindings.decls().iter().find_map(|declaration| {
            match DeclarationRef::from(declaration) {
                DeclarationRef::Class(class) if class.id() == id => Some(class),
                DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            }
        })
    }

    /// Returns the callback declaration with the given id.
    pub fn callback(&self, id: CallbackId) -> Option<&'bindings CallbackDecl<S>> {
        self.bindings.decls().iter().find_map(|declaration| {
            match DeclarationRef::from(declaration) {
                DeclarationRef::Callback(callback) if callback.id() == id => Some(callback),
                DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            }
        })
    }

    /// Returns the stream declaration with the given id.
    pub fn stream(&self, id: StreamId) -> Option<&'bindings StreamDecl<S>> {
        self.bindings.decls().iter().find_map(|declaration| {
            match DeclarationRef::from(declaration) {
                DeclarationRef::Stream(stream) if stream.id() == id => Some(stream),
                DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            }
        })
    }

    /// Returns the constant declaration with the given id.
    pub fn constant(&self, id: ConstantId) -> Option<&'bindings ConstantDecl<S>> {
        self.bindings.decls().iter().find_map(|declaration| {
            match DeclarationRef::from(declaration) {
                DeclarationRef::Constant(constant) if constant.id() == id => Some(constant),
                DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            }
        })
    }

    /// Returns the function declaration with the given id.
    pub fn function(&self, id: FunctionId) -> Option<&'bindings FunctionDecl<S>> {
        self.bindings.decls().iter().find_map(|declaration| {
            match DeclarationRef::from(declaration) {
                DeclarationRef::Function(function) if function.id() == id => Some(function),
                DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            }
        })
    }

    /// Returns the custom type declaration with the given id.
    pub fn custom_type(&self, id: CustomTypeId) -> Option<&'bindings CustomTypeDecl> {
        self.bindings.decls().iter().find_map(|declaration| {
            match DeclarationRef::from(declaration) {
                DeclarationRef::CustomType(custom_type) if custom_type.id() == id => {
                    Some(custom_type)
                }
                DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            }
        })
    }
}

use boltffi_binding::{
    Bindings, CallbackDecl, CallbackId, ClassDecl, ClassId, ConstantDecl, ConstantId,
    CustomTypeDecl, CustomTypeId, DeclarationId, DeclarationRef, EnumDecl, EnumId, FunctionDecl,
    FunctionId, RecordDecl, RecordId, StreamDecl, StreamId, Surface,
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
        self.find(DeclarationId::Record(id), |declaration| match declaration {
            DeclarationRef::Record(record) => Some(record),
            _ => None,
        })
    }

    /// Returns the enum declaration with the given id.
    pub fn enumeration(&self, id: EnumId) -> Option<&'bindings EnumDecl<S>> {
        self.find(DeclarationId::Enum(id), |declaration| match declaration {
            DeclarationRef::Enum(enumeration) => Some(enumeration),
            _ => None,
        })
    }

    /// Returns the class declaration with the given id.
    pub fn class(&self, id: ClassId) -> Option<&'bindings ClassDecl<S>> {
        self.find(DeclarationId::Class(id), |declaration| match declaration {
            DeclarationRef::Class(class) => Some(class),
            _ => None,
        })
    }

    /// Returns the callback declaration with the given id.
    pub fn callback(&self, id: CallbackId) -> Option<&'bindings CallbackDecl<S>> {
        self.find(
            DeclarationId::Callback(id),
            |declaration| match declaration {
                DeclarationRef::Callback(callback) => Some(callback),
                _ => None,
            },
        )
    }

    /// Returns the stream declaration with the given id.
    pub fn stream(&self, id: StreamId) -> Option<&'bindings StreamDecl<S>> {
        self.find(DeclarationId::Stream(id), |declaration| match declaration {
            DeclarationRef::Stream(stream) => Some(stream),
            _ => None,
        })
    }

    /// Returns the constant declaration with the given id.
    pub fn constant(&self, id: ConstantId) -> Option<&'bindings ConstantDecl<S>> {
        self.find(
            DeclarationId::Constant(id),
            |declaration| match declaration {
                DeclarationRef::Constant(constant) => Some(constant),
                _ => None,
            },
        )
    }

    /// Returns the function declaration with the given id.
    pub fn function(&self, id: FunctionId) -> Option<&'bindings FunctionDecl<S>> {
        self.find(
            DeclarationId::Function(id),
            |declaration| match declaration {
                DeclarationRef::Function(function) => Some(function),
                _ => None,
            },
        )
    }

    /// Returns the custom type declaration with the given id.
    pub fn custom_type(&self, id: CustomTypeId) -> Option<&'bindings CustomTypeDecl> {
        self.find(
            DeclarationId::CustomType(id),
            |declaration| match declaration {
                DeclarationRef::CustomType(custom_type) => Some(custom_type),
                _ => None,
            },
        )
    }

    fn find<T>(
        &self,
        id: DeclarationId,
        select: impl Fn(DeclarationRef<'bindings, S>) -> Option<&'bindings T>,
    ) -> Option<&'bindings T> {
        self.bindings.decls().iter().find_map(|declaration| {
            (declaration.id() == id)
                .then(|| select(DeclarationRef::from(declaration)))
                .flatten()
        })
    }
}

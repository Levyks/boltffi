use boltffi_ast::{
    ClassDef, ConstantDef, CustomTypeDef, DeclarationId as SourceDeclarationId, EnumDef,
    FunctionDef, RecordDef, StreamDef, TraitDef,
};
use boltffi_binding::{
    CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, Decl, EnumDecl, FunctionDecl,
    RecordDecl, StreamDecl, Surface,
};

use super::error::Error;

pub enum SourceDeclaration<'a> {
    Record(&'a RecordDef),
    Enum(&'a EnumDef),
    Function(&'a FunctionDef),
    Class(&'a ClassDef),
    Callback(&'a TraitDef),
    Stream(&'a StreamDef),
    Constant(&'a ConstantDef),
    CustomType(&'a CustomTypeDef),
}

impl<'a> SourceDeclaration<'a> {
    pub fn id(&self) -> SourceDeclarationId {
        match self {
            Self::Record(source) => SourceDeclarationId::Record(source.id.clone()),
            Self::Enum(source) => SourceDeclarationId::Enum(source.id.clone()),
            Self::Function(source) => SourceDeclarationId::Function(source.id.clone()),
            Self::Class(source) => SourceDeclarationId::Class(source.id.clone()),
            Self::Callback(source) => SourceDeclarationId::Trait(source.id.clone()),
            Self::Stream(source) => SourceDeclarationId::Stream(source.id.clone()),
            Self::Constant(source) => SourceDeclarationId::Constant(source.id.clone()),
            Self::CustomType(source) => SourceDeclarationId::CustomType(source.id.clone()),
        }
    }

    pub fn pair<S: Surface>(self, binding: &'a Decl<S>) -> Result<PairedDeclaration<'a, S>, Error> {
        match (self, binding) {
            (Self::Record(source), Decl::Record(binding)) => Ok(PairedDeclaration::Record(
                DeclarationPair::new(source, binding.as_ref()),
            )),
            (Self::Enum(source), Decl::Enum(binding)) => Ok(PairedDeclaration::Enum(
                DeclarationPair::new(source, binding.as_ref()),
            )),
            (Self::Function(source), Decl::Function(binding)) => Ok(PairedDeclaration::Function(
                DeclarationPair::new(source, binding.as_ref()),
            )),
            (Self::Class(source), Decl::Class(binding)) => Ok(PairedDeclaration::Class(
                DeclarationPair::new(source, binding.as_ref()),
            )),
            (Self::Callback(source), Decl::Callback(binding)) => Ok(PairedDeclaration::Callback(
                DeclarationPair::new(source, binding.as_ref()),
            )),
            (Self::Stream(source), Decl::Stream(binding)) => Ok(PairedDeclaration::Stream(
                DeclarationPair::new(source, binding.as_ref()),
            )),
            (Self::Constant(source), Decl::Constant(binding)) => Ok(PairedDeclaration::Constant(
                DeclarationPair::new(source, binding.as_ref()),
            )),
            (Self::CustomType(source), Decl::CustomType(binding)) => Ok(
                PairedDeclaration::CustomType(DeclarationPair::new(source, binding.as_ref())),
            ),
            _ => Err(Error::WrongDeclaration),
        }
    }
}

pub enum PairedDeclaration<'a, S: Surface> {
    Record(DeclarationPair<'a, RecordDef, RecordDecl<S>>),
    Enum(DeclarationPair<'a, EnumDef, EnumDecl<S>>),
    Function(DeclarationPair<'a, FunctionDef, FunctionDecl<S>>),
    Class(DeclarationPair<'a, ClassDef, ClassDecl<S>>),
    Callback(DeclarationPair<'a, TraitDef, CallbackDecl<S>>),
    Stream(DeclarationPair<'a, StreamDef, StreamDecl<S>>),
    Constant(DeclarationPair<'a, ConstantDef, ConstantDecl<S>>),
    CustomType(DeclarationPair<'a, CustomTypeDef, CustomTypeDecl>),
}

pub struct DeclarationPair<'a, Source, Binding> {
    source: &'a Source,
    binding: &'a Binding,
}

impl<'a, Source, Binding> DeclarationPair<'a, Source, Binding> {
    pub fn new(source: &'a Source, binding: &'a Binding) -> Self {
        Self { source, binding }
    }

    pub fn source(&self) -> &'a Source {
        self.source
    }

    pub fn binding(&self) -> &'a Binding {
        self.binding
    }
}

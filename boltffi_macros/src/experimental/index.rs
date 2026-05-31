use std::collections::HashMap;

use boltffi_ast::{
    ClassDef, ConstantDef, CustomTypeDef, DeclarationId as SourceDeclarationId, EnumDef,
    FunctionDef, RecordDef, StreamDef, TraitDef,
};
use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, Decl, DeclarationId, EnumDecl,
    FunctionDecl, LoweredBindings, RecordDecl, StreamDecl, Surface,
};

use super::error::Error;

pub struct ExpansionIndex {
    binding_by_id: HashMap<DeclarationId, usize>,
}

impl ExpansionIndex {
    pub fn new<S: Surface>(bindings: &Bindings<S>) -> Self {
        Self {
            binding_by_id: bindings
                .decls()
                .iter()
                .enumerate()
                .map(|(index, decl)| (decl.id(), index))
                .collect(),
        }
    }

    pub fn decl<'a, S: Surface>(
        &self,
        lowered: &'a LoweredBindings<S>,
        source: SourceDecl<'a>,
    ) -> Result<PairedDecl<'a, S>, Error> {
        let source_id = source.id();
        let binding_id = lowered
            .declarations()
            .get(&source_id)
            .ok_or_else(|| Error::MissingBinding(source_id.clone()))?;
        let binding_index = self
            .binding_by_id
            .get(&binding_id)
            .copied()
            .ok_or(Error::MissingDeclaration(binding_id))?;
        let binding = lowered
            .bindings()
            .decls()
            .get(binding_index)
            .ok_or(Error::MissingDeclaration(binding_id))?;
        source.pair(binding)
    }
}

pub enum SourceDecl<'a> {
    Record(&'a RecordDef),
    Enum(&'a EnumDef),
    Function(&'a FunctionDef),
    Class(&'a ClassDef),
    Callback(&'a TraitDef),
    Stream(&'a StreamDef),
    Constant(&'a ConstantDef),
    CustomType(&'a CustomTypeDef),
}

impl<'a> SourceDecl<'a> {
    fn id(&self) -> SourceDeclarationId {
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

    fn pair<S: Surface>(self, binding: &'a Decl<S>) -> Result<PairedDecl<'a, S>, Error> {
        match (self, binding) {
            (Self::Record(source), Decl::Record(binding)) => {
                Ok(PairedDecl::Record(DeclPair::new(source, binding.as_ref())))
            }
            (Self::Enum(source), Decl::Enum(binding)) => {
                Ok(PairedDecl::Enum(DeclPair::new(source, binding.as_ref())))
            }
            (Self::Function(source), Decl::Function(binding)) => Ok(PairedDecl::Function(
                DeclPair::new(source, binding.as_ref()),
            )),
            (Self::Class(source), Decl::Class(binding)) => {
                Ok(PairedDecl::Class(DeclPair::new(source, binding.as_ref())))
            }
            (Self::Callback(source), Decl::Callback(binding)) => Ok(PairedDecl::Callback(
                DeclPair::new(source, binding.as_ref()),
            )),
            (Self::Stream(source), Decl::Stream(binding)) => {
                Ok(PairedDecl::Stream(DeclPair::new(source, binding.as_ref())))
            }
            (Self::Constant(source), Decl::Constant(binding)) => Ok(PairedDecl::Constant(
                DeclPair::new(source, binding.as_ref()),
            )),
            (Self::CustomType(source), Decl::CustomType(binding)) => Ok(PairedDecl::CustomType(
                DeclPair::new(source, binding.as_ref()),
            )),
            _ => Err(Error::WrongDeclaration),
        }
    }
}

pub enum PairedDecl<'a, S: Surface> {
    Record(DeclPair<'a, RecordDef, RecordDecl<S>>),
    Enum(DeclPair<'a, EnumDef, EnumDecl<S>>),
    Function(DeclPair<'a, FunctionDef, FunctionDecl<S>>),
    Class(DeclPair<'a, ClassDef, ClassDecl<S>>),
    Callback(DeclPair<'a, TraitDef, CallbackDecl<S>>),
    Stream(DeclPair<'a, StreamDef, StreamDecl<S>>),
    Constant(DeclPair<'a, ConstantDef, ConstantDecl<S>>),
    CustomType(DeclPair<'a, CustomTypeDef, CustomTypeDecl>),
}

pub struct DeclPair<'a, SourceDecl, BindingDecl> {
    source: &'a SourceDecl,
    binding: &'a BindingDecl,
}

impl<'a, SourceDecl, BindingDecl> DeclPair<'a, SourceDecl, BindingDecl> {
    pub fn new(source: &'a SourceDecl, binding: &'a BindingDecl) -> Self {
        Self { source, binding }
    }

    pub fn source(&self) -> &'a SourceDecl {
        self.source
    }

    pub fn binding(&self) -> &'a BindingDecl {
        self.binding
    }
}

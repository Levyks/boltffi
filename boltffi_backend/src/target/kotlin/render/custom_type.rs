use askama::Template as AskamaTemplate;
use boltffi_binding::{CustomTypeDecl, Native};

use crate::{
    core::{Emitted, RenderContext, Result},
    target::kotlin::{name_style::Name, render::type_name::KotlinType, syntax::TypeName},
};

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/custom_type.kt", escape = "none")]
struct CustomTypeTemplate {
    custom_type: CustomType,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomType {
    name: TypeName,
    representation: TypeName,
}

impl CustomType {
    pub fn from_declaration(
        declaration: &CustomTypeDecl,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Ok(Self {
            name: Name::new(declaration.name()).type_name(),
            representation: KotlinType::type_ref(declaration.representation(), context)?,
        })
    }

    pub fn render(self) -> Result<Emitted> {
        Ok(Emitted::primary(
            CustomTypeTemplate { custom_type: self }.render()?,
        ))
    }

    pub fn name(&self) -> &TypeName {
        &self.name
    }

    pub fn representation(&self) -> &TypeName {
        &self.representation
    }
}

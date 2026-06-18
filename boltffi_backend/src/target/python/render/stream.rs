use boltffi_binding::{
    ByteSize, Native, ReadPlan, StreamDecl, StreamItemPlan, StreamItemPlanRender, TypeRef, native,
};

use crate::{
    core::Result,
    target::python::{
        codec::Expression as CodecExpression, cpython::render::stream as stream_render,
        name_style::Name, render::Package,
    },
};

use super::type_hint::TypeHint;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassStream {
    pub python_name: String,
    pub subscribe_method: String,
    pub subscription_class: String,
    pub item_annotation: String,
    pub pop_batch_body: Vec<String>,
    pub wait_method: String,
    pub unsubscribe_method: String,
    pub free_method: String,
    uses_wire_helpers: bool,
}

impl ClassStream {
    pub fn from_declaration(
        declaration: &StreamDecl<Native>,
        class_name: &str,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        let symbols = stream_render::Symbols::new(declaration);
        let item = StreamItem::from_plan(declaration.item(), package)?;
        let pop_batch_body = item.pop_batch_body(symbols.pop_batch());
        let uses_wire_helpers = item.uses_wire_helpers;
        Ok(Self {
            python_name: Name::new(declaration.name()).function(),
            subscribe_method: symbols.subscribe(),
            subscription_class: format!(
                "{}{}Subscription",
                class_name,
                Name::new(declaration.name()).class()
            ),
            item_annotation: item.annotation,
            pop_batch_body,
            wait_method: symbols.wait(),
            unsubscribe_method: symbols.unsubscribe(),
            free_method: symbols.free(),
            uses_wire_helpers,
        })
    }

    pub fn uses_wire_helpers(&self) -> bool {
        self.uses_wire_helpers
    }

    pub fn member_name(&self) -> (String, String) {
        (
            self.python_name.clone(),
            format!("stream `{}`", self.python_name),
        )
    }

    pub fn top_level_name(&self) -> (String, String) {
        (
            self.subscription_class.clone(),
            format!("stream subscription `{}`", self.subscription_class),
        )
    }
}

struct StreamItem {
    annotation: String,
    decode: Option<String>,
    uses_wire_helpers: bool,
}

impl StreamItem {
    fn from_plan(plan: &StreamItemPlan<Native>, package: &Package<'_, '_>) -> Result<Self> {
        plan.render_with(&mut PackageStreamItem { package })
    }

    fn pop_batch_body(&self, method: String) -> Vec<String> {
        match &self.decode {
            Some(decode) => vec![
                format!("data = _native.{method}(self._require_handle(), max_count)"),
                format!("return _boltffi_read_wire(data, lambda reader: {decode}) if data else []"),
            ],
            None => vec![format!(
                "return _native.{method}(self._require_handle(), max_count)"
            )],
        }
    }
}

struct PackageStreamItem<'package, 'binding, 'bridge> {
    package: &'package Package<'binding, 'bridge>,
}

impl<'plan> StreamItemPlanRender<'plan, Native> for PackageStreamItem<'_, '_, '_> {
    type Output = Result<StreamItem>;

    fn direct(&mut self, ty: &'plan TypeRef, _: ByteSize) -> Self::Output {
        Ok(StreamItem {
            annotation: TypeHint::from_type_ref(ty, self.package)?.into_string(),
            decode: None,
            uses_wire_helpers: false,
        })
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        read: &'plan ReadPlan,
        _: native::BufferShape,
    ) -> Self::Output {
        Ok(StreamItem {
            annotation: TypeHint::from_type_ref(ty, self.package)?.into_string(),
            decode: Some(CodecExpression::read_sequence(read, self.package)?.into_string()),
            uses_wire_helpers: true,
        })
    }
}

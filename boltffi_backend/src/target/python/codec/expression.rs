use boltffi_binding::{ReadPlan, WritePlan};

use crate::{
    core::Result,
    target::python::{
        codec::{read::Reader, write::Writer},
        render::Package,
        syntax::Expression as PythonExpression,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Expression {
    expression: PythonExpression,
}

impl Expression {
    pub fn read<'package>(plan: &ReadPlan, package: &'package Package<'package>) -> Result<Self> {
        let mut reader = Reader::new(package);
        Ok(Self {
            expression: plan.render_with(&mut reader)?,
        })
    }

    pub fn read_sequence<'package>(
        item: &ReadPlan,
        package: &'package Package<'package>,
    ) -> Result<Self> {
        let mut reader = Reader::new(package);
        let item = item.render_with(&mut reader)?;
        Ok(Self {
            expression: reader.sequence_expression(item)?,
        })
    }

    pub fn write<'package>(plan: &WritePlan, package: &'package Package<'package>) -> Result<Self> {
        let mut writer = Writer::new(package);
        Ok(Self {
            expression: Writer::single(plan.render_with(&mut writer))?,
        })
    }

    pub fn write_argument<'package>(
        plan: &WritePlan,
        package: &'package Package<'package>,
    ) -> Result<Self> {
        Self::write(plan, package)
    }

    pub fn read_return<'package>(
        plan: &ReadPlan,
        package: &'package Package<'package>,
    ) -> Result<Self> {
        Self::read(plan, package)
    }

    pub fn into_expression(self) -> PythonExpression {
        self.expression
    }
}

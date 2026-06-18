use boltffi_binding::{ReadPlan, WritePlan};

use crate::{
    core::Result,
    target::python::{
        codec::{read::Reader, write::Writer},
        render::Package,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Expression {
    expression: String,
}

impl Expression {
    pub fn read(plan: &ReadPlan, package: &Package<'_, '_>) -> Result<Self> {
        let mut reader = Reader::new(package);
        Ok(Self {
            expression: plan.render_with(&mut reader)?,
        })
    }

    pub fn read_sequence(item: &ReadPlan, package: &Package<'_, '_>) -> Result<Self> {
        let mut reader = Reader::new(package);
        let item = item.render_with(&mut reader)?;
        Ok(Self {
            expression: format!("reader.sequence(lambda: {item})"),
        })
    }

    pub fn write(plan: &WritePlan, package: &Package<'_, '_>) -> Result<Self> {
        let mut writer = Writer::new(package);
        Ok(Self {
            expression: Writer::single(plan.render_with(&mut writer))?,
        })
    }

    pub fn write_argument(plan: &WritePlan, package: &Package<'_, '_>) -> Result<Self> {
        Self::write(plan, package)
    }

    pub fn read_return(plan: &ReadPlan, package: &Package<'_, '_>) -> Result<Self> {
        Self::read(plan, package)
    }

    pub fn into_string(self) -> String {
        self.expression
    }
}

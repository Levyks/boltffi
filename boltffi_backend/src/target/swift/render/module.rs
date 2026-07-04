use askama::Template;

use boltffi_binding::Native;

use crate::{
    core::{FileLayout, FilePath, FilePlan, GeneratedOutput, RenderedDeclaration, Result},
    target::swift::SwiftHost,
};

#[derive(Template)]
#[template(path = "target/swift/module.swift", escape = "none")]
struct ModuleTemplate<'a> {
    module: &'a str,
}

pub struct Module<'host, 'decl> {
    host: &'host SwiftHost,
    declarations: Vec<RenderedDeclaration<'decl, Native>>,
}

impl<'host, 'decl> Module<'host, 'decl> {
    pub fn new(
        host: &'host SwiftHost,
        declarations: Vec<RenderedDeclaration<'decl, Native>>,
    ) -> Self {
        Self { host, declarations }
    }

    pub fn render(self) -> Result<GeneratedOutput> {
        let mut preamble = ModuleTemplate {
            module: self.host.module().as_str(),
        }
        .render()?;
        preamble.push('\n');
        let file =
            FilePlan::all(FilePath::new(self.host.file_name().path())?).with_preamble(preamble);
        FileLayout::new()
            .with_file(file)
            .assemble_declarations(self.declarations)
    }
}

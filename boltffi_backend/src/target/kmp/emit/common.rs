//! commonMain Kotlin source rendering for KMP emission.

use askama::Template as AskamaTemplate;

use crate::core::Result;

use super::super::plan::KmpModule;

#[derive(AskamaTemplate)]
#[template(path = "target/kmp/common_module.kt", escape = "none")]
struct CommonModuleTemplate<'options> {
    package_name: &'options str,
}

pub(crate) fn render_common_module(module: &KmpModule, package_name: &str) -> Result<String> {
    let _ = module;
    Ok(CommonModuleTemplate { package_name }.render()?)
}

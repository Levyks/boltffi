//! JVM-family source-set file rendering for KMP emission.

use askama::Template as AskamaTemplate;

use crate::core::Result;

#[derive(AskamaTemplate)]
#[template(path = "target/kmp/package_source.kt", escape = "none")]
struct PackageSourceTemplate<'options> {
    package_name: &'options str,
}

#[derive(AskamaTemplate)]
#[template(path = "target/kmp/jni_glue.c", escape = "none")]
struct JniGlueTemplate;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct KmpJvmAdapter {
    pub(crate) source_set: &'static str,
    pub(crate) actual_file_suffix: &'static str,
}

impl KmpJvmAdapter {
    pub(crate) const fn jvm() -> Self {
        Self {
            source_set: "jvmMain",
            actual_file_suffix: "JvmActual",
        }
    }

    pub(crate) const fn android() -> Self {
        Self {
            source_set: "androidMain",
            actual_file_suffix: "AndroidActual",
        }
    }
}

pub(crate) fn default_adapters() -> Vec<KmpJvmAdapter> {
    vec![KmpJvmAdapter::jvm(), KmpJvmAdapter::android()]
}

pub(crate) fn render_platform_actual(package_name: &str) -> Result<String> {
    Ok(PackageSourceTemplate { package_name }.render()?)
}

pub(crate) fn render_internal_kotlin(internal_package: &str) -> Result<String> {
    Ok(PackageSourceTemplate {
        package_name: internal_package,
    }
    .render()?)
}

pub(crate) fn render_jni_glue() -> Result<String> {
    Ok(JniGlueTemplate.render()?)
}

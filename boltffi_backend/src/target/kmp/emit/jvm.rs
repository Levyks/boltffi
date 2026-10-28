//! JVM-family source-set file rendering for KMP emission.

use askama::Template as AskamaTemplate;

use crate::{
    core::Result,
    target::{jvm::NativeLibraries, kotlin::render::native_library_loader::NativeLibraryLoader},
};

use super::common::RenderedFunction;

#[derive(AskamaTemplate)]
#[template(path = "target/kmp/platform_actual.kt", escape = "none")]
struct PlatformActualTemplate<'module> {
    package_name: &'module str,
    internal_package: &'module str,
    functions: &'module [RenderedFunction],
}

#[derive(AskamaTemplate)]
#[template(path = "target/kmp/internal_kotlin.kt", escape = "none")]
struct InternalKotlinTemplate<'module> {
    internal_package: &'module str,
    native_library_loader: String,
    functions: &'module [RenderedFunction],
}

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

pub fn default_adapters() -> [KmpJvmAdapter; 2] {
    [KmpJvmAdapter::jvm(), KmpJvmAdapter::android()]
}

pub fn render_platform_actual(
    functions: &[RenderedFunction],
    package_name: &str,
    internal_package: &str,
) -> Result<String> {
    Ok(PlatformActualTemplate {
        package_name,
        internal_package,
        functions,
    }
    .render()?)
}

pub fn render_internal_kotlin(
    functions: &[RenderedFunction],
    internal_package: &str,
    native_libraries: &NativeLibraries,
) -> Result<String> {
    Ok(InternalKotlinTemplate {
        internal_package,
        native_library_loader: NativeLibraryLoader::new(native_libraries).render()?,
        functions,
    }
    .render()?)
}

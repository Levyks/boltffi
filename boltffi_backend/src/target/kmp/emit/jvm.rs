//! JVM-family source-set file rendering for KMP emission.

use askama::Template as AskamaTemplate;

use crate::core::{Error, Result};

use super::{
    super::plan::{KmpApiBody, KmpFunctionPlan, KmpJvmDelegateOutput, KmpModule},
    common::{RenderedFunction, unsupported_body_emission},
};

#[derive(AskamaTemplate)]
#[template(path = "target/kmp/platform_actual.kt", escape = "none")]
struct PlatformActualTemplate<'module> {
    package_name: &'module str,
    internal_package: &'module str,
    functions: Vec<RenderedFunction>,
}

#[derive(AskamaTemplate)]
#[template(path = "target/kmp/internal_kotlin.kt", escape = "none")]
struct InternalKotlinTemplate<'module> {
    internal_package: &'module str,
    runtime: Option<&'module str>,
    functions: Vec<RenderedFunction>,
}

#[derive(AskamaTemplate)]
#[template(path = "target/kmp/jni_glue.c", escape = "none")]
struct JniGlueTemplate<'module> {
    delegate_functions: Vec<&'module str>,
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

pub(crate) fn default_adapters() -> Vec<KmpJvmAdapter> {
    vec![KmpJvmAdapter::jvm(), KmpJvmAdapter::android()]
}

pub(crate) fn render_platform_actual(
    module: &KmpModule,
    package_name: &str,
    internal_package: &str,
) -> Result<String> {
    let functions = function_plans(module)?;
    if !functions.is_empty() {
        delegate_for_functions(module, &functions, Some(internal_package))?;
    }

    Ok(PlatformActualTemplate {
        package_name,
        internal_package,
        functions: rendered_functions(&functions)?,
    }
    .render()?)
}

pub(crate) fn render_internal_kotlin(module: &KmpModule, internal_package: &str) -> Result<String> {
    let functions = function_plans(module)?;
    let delegate = (!functions.is_empty())
        .then(|| delegate_for_functions(module, &functions, Some(internal_package)))
        .transpose()?;

    Ok(InternalKotlinTemplate {
        internal_package,
        runtime: delegate.map(KmpJvmDelegateOutput::internal_kotlin_runtime_source),
        functions: rendered_functions(&functions)?,
    }
    .render()?)
}

pub(crate) fn render_jni_glue(module: &KmpModule) -> Result<String> {
    let functions = function_plans(module)?;
    let delegate_functions = if functions.is_empty() {
        Vec::new()
    } else {
        let delegate = delegate_for_functions(module, &functions, None)?;
        functions
            .iter()
            .map(|function| {
                delegate_function_for(delegate, function).map(|function| function.jni_glue_source())
            })
            .collect::<Result<Vec<_>>>()?
    };

    Ok(JniGlueTemplate { delegate_functions }.render()?)
}

fn function_plans(module: &KmpModule) -> Result<Vec<&KmpFunctionPlan>> {
    module
        .common()
        .apis()
        .iter()
        .map(|api| match api.body() {
            KmpApiBody::Function(function) => Ok(function),
            KmpApiBody::Unsupported => Err(unsupported_body_emission()),
        })
        .collect()
}

fn rendered_functions(functions: &[&KmpFunctionPlan]) -> Result<Vec<RenderedFunction>> {
    functions
        .iter()
        .map(|function| RenderedFunction::from_plan(function))
        .collect()
}

fn delegate_for_functions<'module>(
    module: &'module KmpModule,
    functions: &[&KmpFunctionPlan],
    internal_package: Option<&str>,
) -> Result<&'module KmpJvmDelegateOutput> {
    let delegate = module.jvm_delegate().ok_or(Error::UnsupportedTarget {
        target: "kotlin_multiplatform",
        shape: "KMP JNI glue emission",
    })?;
    if internal_package.is_some_and(|expected| delegate.internal_package() != expected) {
        return Err(Error::UnsupportedTarget {
            target: "kotlin_multiplatform",
            shape: "KMP JNI glue emission",
        });
    }
    if functions
        .iter()
        .all(|function| delegate.covers_function(function))
    {
        Ok(delegate)
    } else {
        Err(Error::UnsupportedTarget {
            target: "kotlin_multiplatform",
            shape: "KMP JNI glue emission",
        })
    }
}

fn delegate_function_for<'delegate>(
    delegate: &'delegate KmpJvmDelegateOutput,
    function: &KmpFunctionPlan,
) -> Result<&'delegate super::super::plan::KmpJvmDelegateFunction> {
    delegate
        .function_for(function)
        .ok_or(Error::UnsupportedTarget {
            target: "kotlin_multiplatform",
            shape: "KMP JNI glue emission",
        })
}

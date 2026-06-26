use std::collections::HashMap;

use boltffi_backend::target::kmp::{KmpJvmDelegateFunction, KmpJvmDelegateOutput, KmpTypePlan};
use boltffi_binding::Primitive as BackendPrimitive;

use crate::ir::FfiContract;
use crate::ir::abi::{AbiContract, CallId, CallMode};
use crate::ir::definitions::{FunctionDef, ParamPassing, ReturnDef};
use crate::ir::types::{PrimitiveType, TypeExpr};
use crate::render::jni::{JniEmitter, JniFunction, JniLowerer, JniModule, JvmBindingStyle};
use crate::render::kmp::{
    KmpSurfaceSupport, filter_abi_for_kmp_surface, filter_contract_for_kmp_surface,
};
use crate::render::kotlin::{KotlinEmitter, KotlinLowerer, KotlinModule, KotlinOptions};

#[derive(Debug, thiserror::Error)]
pub(crate) enum KmpJvmDelegateAdapterError {
    #[error("Kotlin/JNI delegate source did not contain the Native object")]
    MissingNativeObject,
    #[error("JNI function source for {jni_name} was not isolated from shared source")]
    MissingJniFunction { jni_name: String },
}

pub(crate) struct KmpJvmDelegateAdapter {
    package_name: String,
    module_name: String,
    kotlin_options: KotlinOptions,
}

impl KmpJvmDelegateAdapter {
    pub(crate) fn new(
        package_name: impl Into<String>,
        module_name: impl Into<String>,
        kotlin_options: KotlinOptions,
    ) -> Self {
        Self {
            package_name: package_name.into(),
            module_name: module_name.into(),
            kotlin_options,
        }
    }

    pub(crate) fn adapt(
        &self,
        contract: &FfiContract,
        abi: &AbiContract,
    ) -> Result<KmpJvmDelegateOutput, KmpJvmDelegateAdapterError> {
        let internal_package = format!("{}.jvm", self.package_name);
        let support = KmpSurfaceSupport::for_contract(contract);
        let internal_contract = filter_contract_for_kmp_delegate_surface(contract, &support);
        let internal_abi = filter_abi_for_kmp_surface(&internal_contract, abi, &support);

        let kotlin_module = KotlinLowerer::new(
            &internal_contract,
            &internal_abi,
            internal_package.clone(),
            self.module_name.clone(),
            self.kotlin_options.clone(),
        )
        .lower();
        let jni_module = JniLowerer::new(
            &internal_contract,
            &internal_abi,
            internal_package.clone(),
            self.module_name.clone(),
        )
        .with_jvm_binding_style(JvmBindingStyle::Kotlin)
        .lower();

        let runtime_source = native_runtime_members(&kotlin_module)?;
        let shared_jni_source = shared_jni_source(&jni_module);
        let functions = delegate_functions(
            &internal_contract,
            &internal_abi,
            &kotlin_module,
            &jni_module,
            &shared_jni_source,
        )?;

        Ok(
            KmpJvmDelegateOutput::new(internal_package, runtime_source, functions)
                .with_shared_jni_source(shared_jni_source),
        )
    }
}

fn delegate_functions(
    contract: &FfiContract,
    abi: &AbiContract,
    kotlin_module: &KotlinModule,
    jni_module: &JniModule,
    shared_jni_source: &str,
) -> Result<Vec<KmpJvmDelegateFunction>, KmpJvmDelegateAdapterError> {
    let function_defs = function_defs_by_symbol(contract, abi);
    let native_functions = kotlin_module
        .native
        .functions
        .iter()
        .filter(|function| !function.is_async())
        .map(|function| (function.ffi_name.as_str(), function))
        .collect::<HashMap<_, _>>();
    let jni_functions = jni_module
        .functions
        .iter()
        .map(|function| (function.ffi_name.as_str(), function))
        .collect::<HashMap<_, _>>();
    let mut delegates = Vec::new();

    for kotlin_function in &kotlin_module.functions {
        if kotlin_function.is_async() {
            continue;
        }
        let Some(function_def) = function_defs.get(kotlin_function.ffi_name.as_str()) else {
            continue;
        };
        let Some((param_types, returns)) = kmp_function_signature(function_def) else {
            continue;
        };
        if !native_functions.contains_key(kotlin_function.ffi_name.as_str()) {
            continue;
        }
        let Some(jni_function) = jni_functions.get(kotlin_function.ffi_name.as_str()) else {
            continue;
        };
        let native_symbol = kmp_native_function_symbol(contract, function_def);
        delegates.push(KmpJvmDelegateFunction::new(
            native_symbol.clone(),
            kotlin_function.func_name.clone(),
            param_types,
            returns,
            jni_function_source(jni_module, jni_function, &native_symbol, shared_jni_source)?,
        ));
    }

    Ok(delegates)
}

fn filter_contract_for_kmp_delegate_surface(
    contract: &FfiContract,
    support: &KmpSurfaceSupport,
) -> FfiContract {
    let mut contract = filter_contract_for_kmp_surface(contract, support);
    contract
        .functions
        .retain(|function| kmp_function_signature(function).is_some());
    contract
}

fn function_defs_by_symbol<'contract>(
    contract: &'contract FfiContract,
    abi: &'contract AbiContract,
) -> HashMap<&'contract str, &'contract FunctionDef> {
    contract
        .functions
        .iter()
        .filter_map(|function| {
            let symbol = abi
                .calls
                .iter()
                .find_map(|call| match (&call.id, &call.mode) {
                    (CallId::Function(id), CallMode::Sync) if id == &function.id => {
                        Some(call.symbol.as_str())
                    }
                    _ => None,
                })?;
            Some((symbol, function))
        })
        .collect()
}

fn kmp_function_signature(
    function: &FunctionDef,
) -> Option<(Vec<KmpTypePlan>, Option<KmpTypePlan>)> {
    if function.is_async() {
        return None;
    }
    let params = function
        .params
        .iter()
        .map(|param| match param.passing {
            ParamPassing::Value | ParamPassing::Ref => kmp_type_for_type_expr(&param.type_expr),
            ParamPassing::RefMut | ParamPassing::ImplTrait | ParamPassing::BoxedDyn => None,
        })
        .collect::<Option<Vec<_>>>()?;
    let returns = match &function.returns {
        ReturnDef::Void => None,
        ReturnDef::Value(ty) => Some(kmp_type_for_type_expr(ty)?),
        ReturnDef::Result { .. } => return None,
    };

    Some((params, returns))
}

fn kmp_type_for_type_expr(ty: &TypeExpr) -> Option<KmpTypePlan> {
    let TypeExpr::Primitive(primitive) = ty else {
        return None;
    };
    let primitive = match primitive {
        PrimitiveType::Bool => BackendPrimitive::Bool,
        PrimitiveType::I8 => BackendPrimitive::I8,
        PrimitiveType::I16 => BackendPrimitive::I16,
        PrimitiveType::I32 => BackendPrimitive::I32,
        PrimitiveType::I64 => BackendPrimitive::I64,
        PrimitiveType::ISize => BackendPrimitive::ISize,
        PrimitiveType::F32 => BackendPrimitive::F32,
        PrimitiveType::F64 => BackendPrimitive::F64,
        PrimitiveType::U8
        | PrimitiveType::U16
        | PrimitiveType::U32
        | PrimitiveType::U64
        | PrimitiveType::USize => return None,
    };
    Some(KmpTypePlan::Primitive(primitive))
}

fn kmp_native_function_symbol(contract: &FfiContract, function: &FunctionDef) -> String {
    let source_id = if function.id.as_str().contains("::") {
        function.id.as_str().to_string()
    } else {
        format!("{}::{}", contract.package.name, function.id.as_str())
    };
    format!("boltffi_function_{}", source_id_to_symbol_path(&source_id))
}

fn source_id_to_symbol_path(source_id: &str) -> String {
    source_id
        .split("::")
        .filter(|segment| !segment.is_empty())
        .map(to_snake_case)
        .collect::<Vec<_>>()
        .join("_")
}

fn to_snake_case(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    chars
        .iter()
        .enumerate()
        .fold(String::new(), |mut result, (index, &character)| {
            if character.is_uppercase() && index > 0 {
                let previous = chars[index - 1];
                let next = chars.get(index + 1).copied();
                let previous_is_word = previous.is_lowercase() || previous.is_ascii_digit();
                let acronym_word_break = previous.is_uppercase()
                    && next.is_some_and(|character| character.is_lowercase());
                if previous_is_word || acronym_word_break {
                    result.push('_');
                }
            }
            result.extend(character.to_lowercase());
            result
        })
}

fn native_runtime_members(
    kotlin_module: &KotlinModule,
) -> Result<String, KmpJvmDelegateAdapterError> {
    let mut runtime_module = kotlin_module.clone();
    runtime_module.functions.clear();
    runtime_module.classes.clear();
    runtime_module.callbacks.clear();
    runtime_module.native.functions.clear();
    runtime_module.native.wire_functions.clear();
    runtime_module.native.classes.clear();
    runtime_module.native.async_callback_invokers.clear();

    let source = KotlinEmitter::emit(&runtime_module);
    extract_native_object_members(&source)
}

fn extract_native_object_members(source: &str) -> Result<String, KmpJvmDelegateAdapterError> {
    let marker = "private object Native {";
    let start = source
        .find(marker)
        .map(|index| index + marker.len())
        .ok_or(KmpJvmDelegateAdapterError::MissingNativeObject)?;
    let body = &source[start..];
    let end = body
        .rfind("\n}")
        .ok_or(KmpJvmDelegateAdapterError::MissingNativeObject)?;
    Ok(unindent_kotlin_members(&body[..end]))
}

fn unindent_kotlin_members(source: &str) -> String {
    let mut out = source
        .lines()
        .map(|line| line.strip_prefix("    ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    out.push('\n');
    out
}

fn shared_jni_source(jni_module: &JniModule) -> String {
    let module = shared_jni_module(jni_module);
    JniEmitter::emit(&module)
}

fn jni_function_source(
    jni_module: &JniModule,
    function: &JniFunction,
    native_symbol: &str,
    shared_jni_source: &str,
) -> Result<String, KmpJvmDelegateAdapterError> {
    let mut module = shared_jni_module(jni_module);
    let renamed_function = renamed_jni_function(function, &module.jni_prefix, native_symbol);
    let jni_name = renamed_function.jni_name.clone();
    module.functions = vec![renamed_function];
    let source = JniEmitter::emit(&module);
    let function_source = source
        .strip_prefix(shared_jni_source)
        .ok_or_else(|| KmpJvmDelegateAdapterError::MissingJniFunction {
            jni_name: jni_name.clone(),
        })?
        .trim();
    if function_source.is_empty() || !function_source.contains(&jni_name) {
        return Err(KmpJvmDelegateAdapterError::MissingJniFunction { jni_name });
    }
    Ok(format!("{function_source}\n"))
}

fn renamed_jni_function(
    function: &JniFunction,
    jni_prefix: &str,
    native_symbol: &str,
) -> JniFunction {
    let mut function = function.clone();
    function.ffi_name = native_symbol.to_string();
    function.jni_name = jni_export_name(jni_prefix, native_symbol);
    function
}

fn jni_export_name(jni_prefix: &str, native_symbol: &str) -> String {
    format!(
        "Java_{}_Native_{}",
        jni_prefix,
        native_symbol.replace('_', "_1")
    )
}

fn shared_jni_module(jni_module: &JniModule) -> JniModule {
    let mut module = jni_module.clone();
    module.functions.clear();
    module.wire_functions.clear();
    module.async_functions.clear();
    module.classes.clear();
    module.callback_traits.clear();
    module.async_callback_invokers.clear();
    module.closure_trampolines.clear();
    module
}

#[cfg(test)]
mod tests {
    use boltffi_backend::target::kmp::{KmpFunctionPlan, KmpParamPlan};

    use super::*;
    use crate::ir::definitions::{ParamDef, ParamPassing};
    use crate::ir::{Lowerer, PackageInfo, TypeCatalog};
    use boltffi_ffi_rules::callable::ExecutionKind;

    fn empty_contract() -> FfiContract {
        FfiContract {
            package: PackageInfo {
                name: "demo".to_string(),
                version: None,
            },
            catalog: TypeCatalog::new(),
            functions: Vec::new(),
        }
    }

    fn sync_primitive_function(
        id: &str,
        params: Vec<(&str, PrimitiveType)>,
        returns: ReturnDef,
    ) -> FunctionDef {
        primitive_function(id, params, returns, ExecutionKind::Sync)
    }

    fn async_primitive_function(
        id: &str,
        params: Vec<(&str, PrimitiveType)>,
        returns: ReturnDef,
    ) -> FunctionDef {
        primitive_function(id, params, returns, ExecutionKind::Async)
    }

    fn primitive_function(
        id: &str,
        params: Vec<(&str, PrimitiveType)>,
        returns: ReturnDef,
        execution_kind: ExecutionKind,
    ) -> FunctionDef {
        FunctionDef {
            id: id.into(),
            params: params
                .into_iter()
                .map(|(name, primitive)| ParamDef {
                    name: name.into(),
                    type_expr: TypeExpr::Primitive(primitive),
                    passing: ParamPassing::Value,
                    doc: None,
                })
                .collect(),
            returns,
            execution_kind,
            doc: None,
            deprecated: None,
        }
    }

    fn adapt(contract: &FfiContract) -> KmpJvmDelegateOutput {
        let abi = Lowerer::new(contract).to_abi_contract();
        KmpJvmDelegateAdapter::new("com.example.demo", "Demo", KotlinOptions::default())
            .adapt(contract, &abi)
            .expect("delegate adapter should render")
    }

    #[test]
    fn adapter_builds_delegate_for_sync_primitive_function() {
        let mut contract = empty_contract();
        contract.functions.push(sync_primitive_function(
            "add",
            vec![("left", PrimitiveType::I32), ("right", PrimitiveType::I32)],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
        ));

        let delegate = adapt(&contract);
        let function_plan = KmpFunctionPlan::new(
            "add",
            "boltffi_function_demo_add",
            vec![
                KmpParamPlan::new("left", KmpTypePlan::Primitive(BackendPrimitive::I32)),
                KmpParamPlan::new("right", KmpTypePlan::Primitive(BackendPrimitive::I32)),
            ],
            Some(KmpTypePlan::Primitive(BackendPrimitive::I32)),
        );
        let function = delegate
            .function_for(&function_plan)
            .expect("primitive function should be covered by the adapter");

        assert_eq!(delegate.internal_package(), "com.example.demo.jvm");
        assert!(
            delegate
                .internal_kotlin_runtime_source()
                .contains("System.loadLibrary(androidLibrary)")
        );
        assert!(
            !delegate
                .internal_kotlin_runtime_source()
                .contains("private object Native")
        );
        assert!(delegate.shared_jni_source().contains("#include <jni.h>"));
        assert!(
            !delegate
                .shared_jni_source()
                .contains("JNIEXPORT jint JNICALL")
        );
        assert!(function.jni_glue_source().contains(
            "JNIEXPORT jint JNICALL Java_com_example_demo_jvm_Native_boltffi_1function_1demo_1add"
        ));
        assert!(
            function
                .jni_glue_source()
                .contains("_result = boltffi_function_demo_add(left, right);")
        );
        assert!(
            !function
                .jni_glue_source()
                .contains("Java_com_example_demo_jvm_Native_boltffi_1add")
        );
        assert!(
            !function
                .jni_glue_source()
                .contains("_result = boltffi_add(left, right);")
        );
        assert!(!function.jni_glue_source().contains("#include <jni.h>"));
    }

    #[test]
    fn adapter_filters_async_functions_before_lowering_delegate_runtime() {
        let mut contract = empty_contract();
        contract.functions.push(sync_primitive_function(
            "add",
            vec![("left", PrimitiveType::I32), ("right", PrimitiveType::I32)],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
        ));
        contract.functions.push(async_primitive_function(
            "spin",
            vec![("value", PrimitiveType::I32)],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
        ));

        let delegate = adapt(&contract);
        let function_plan = KmpFunctionPlan::new(
            "add",
            "boltffi_function_demo_add",
            vec![
                KmpParamPlan::new("left", KmpTypePlan::Primitive(BackendPrimitive::I32)),
                KmpParamPlan::new("right", KmpTypePlan::Primitive(BackendPrimitive::I32)),
            ],
            Some(KmpTypePlan::Primitive(BackendPrimitive::I32)),
        );

        assert!(delegate.covers_function(&function_plan));
        assert!(!delegate.shared_jni_source().contains("JNI_OnLoad"));
        assert!(!delegate.shared_jni_source().contains("g_jvm"));
        assert!(!delegate.shared_jni_source().contains("spin"));
        assert!(
            !delegate
                .internal_kotlin_runtime_source()
                .contains("boltffiFutureContinuationCallback")
        );
        assert!(!delegate.internal_kotlin_runtime_source().contains("spin"));
    }

    #[test]
    fn adapter_covers_immutable_primitive_ref_params_as_direct_primitives() {
        let mut contract = empty_contract();
        contract.functions.push(FunctionDef {
            id: "read".into(),
            params: vec![ParamDef {
                name: "value".into(),
                type_expr: TypeExpr::Primitive(PrimitiveType::I32),
                passing: ParamPassing::Ref,
                doc: None,
            }],
            returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        });

        let delegate = adapt(&contract);
        let function_plan = KmpFunctionPlan::new(
            "read",
            "boltffi_function_demo_read",
            vec![KmpParamPlan::new(
                "value",
                KmpTypePlan::Primitive(BackendPrimitive::I32),
            )],
            Some(KmpTypePlan::Primitive(BackendPrimitive::I32)),
        );
        let function = delegate
            .function_for(&function_plan)
            .expect("immutable primitive ref should be covered as a direct primitive");

        assert!(function.jni_glue_source().contains(
            "JNIEXPORT jint JNICALL Java_com_example_demo_jvm_Native_boltffi_1function_1demo_1read"
        ));
        assert!(
            function
                .jni_glue_source()
                .contains("_result = boltffi_function_demo_read(value);")
        );
    }

    #[test]
    fn adapter_does_not_cover_non_primitive_functions_until_conversion_plan_exists() {
        let mut contract = empty_contract();
        contract.functions.push(FunctionDef {
            id: "load".into(),
            params: vec![ParamDef {
                name: "name".into(),
                type_expr: TypeExpr::String,
                passing: ParamPassing::Value,
                doc: None,
            }],
            returns: ReturnDef::Value(TypeExpr::String),
            execution_kind: boltffi_ffi_rules::callable::ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        });

        let delegate = adapt(&contract);
        let function_plan = KmpFunctionPlan::new("load", "boltffi_load", Vec::new(), None);

        assert!(!delegate.covers_function(&function_plan));
        assert!(delegate.shared_jni_source().contains("#include <jni.h>"));
    }
}

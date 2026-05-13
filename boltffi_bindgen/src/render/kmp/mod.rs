use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::ir::{self, AbiContract, FfiContract};
use crate::render::jni::{JniEmitter, JniLowerer, JvmBindingStyle};
use crate::render::kotlin::{KotlinEmitter, KotlinLowerer, KotlinOptions, NamingConvention};

#[derive(Debug, Clone)]
pub struct KMPOptions {
    pub package_name: String,
    pub module_name: String,
    pub min_sdk: u32,
    pub kotlin_options: KotlinOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KMPOutputFile {
    pub relative_path: PathBuf,
    pub contents: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KMPOutput {
    pub files: Vec<KMPOutputFile>,
}

pub struct KMPEmitter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KmpActualBackend {
    KotlinJvm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KmpPlatformAdapter {
    source_set: &'static str,
    actual_file_suffix: &'static str,
    backend: KmpActualBackend,
}

impl KmpPlatformAdapter {
    const fn jvm() -> Self {
        Self {
            source_set: "jvmMain",
            actual_file_suffix: "JvmActual",
            backend: KmpActualBackend::KotlinJvm,
        }
    }

    const fn android() -> Self {
        Self {
            source_set: "androidMain",
            actual_file_suffix: "AndroidActual",
            backend: KmpActualBackend::KotlinJvm,
        }
    }
}

struct KmpRender {
    common: String,
    platform_actuals: Vec<KmpPlatformActual>,
}

struct KmpPlatformActual {
    adapter: KmpPlatformAdapter,
    contents: String,
}

struct KmpSurfaceSupport {
    records: HashSet<String>,
    enums: HashSet<String>,
    custom_types: HashSet<String>,
}

impl KmpSurfaceSupport {
    fn for_contract(contract: &ir::FfiContract) -> Self {
        let records = supported_records(contract);
        let enums = supported_c_style_enums(contract);
        let custom_types = supported_custom_types(contract, &records, &enums);

        Self {
            records,
            enums,
            custom_types,
        }
    }
}

impl KMPEmitter {
    fn package_path(package_name: &str) -> PathBuf {
        package_name.split('.').collect()
    }

    pub fn emit(contract: &FfiContract, abi: &AbiContract, options: KMPOptions) -> KMPOutput {
        let KMPOptions {
            package_name,
            module_name,
            min_sdk,
            kotlin_options,
        } = options;
        let internal_package = format!("{package_name}.jvm");
        let common_package_path = Self::package_path(&package_name);
        let internal_package_path = Self::package_path(&internal_package);
        let platform_adapters = Self::default_platform_adapters();

        let rendered = Self::render_surfaces(
            contract,
            &package_name,
            &internal_package,
            &platform_adapters,
        );

        let kotlin_module = KotlinLowerer::new(
            contract,
            abi,
            internal_package.clone(),
            module_name.clone(),
            kotlin_options,
        )
        .lower();
        let jvm_source = KotlinEmitter::emit(&kotlin_module);

        let jni_module = JniLowerer::new(contract, abi, internal_package, module_name.clone())
            .with_jvm_binding_style(JvmBindingStyle::Kotlin)
            .lower();
        let jni_source = JniEmitter::emit(&jni_module);

        let common_dir = PathBuf::from("src/commonMain/kotlin").join(&common_package_path);

        let mut files = vec![
            KMPOutputFile {
                relative_path: PathBuf::from("settings.gradle.kts"),
                contents: Self::render_settings_gradle(&module_name),
            },
            KMPOutputFile {
                relative_path: PathBuf::from("build.gradle.kts"),
                contents: Self::render_build_gradle(&package_name, min_sdk),
            },
            KMPOutputFile {
                relative_path: common_dir.join(format!("{module_name}.kt")),
                contents: rendered.common,
            },
        ];

        rendered.platform_actuals.into_iter().for_each(|actual| {
            let actual_dir =
                Self::source_set_kotlin_dir(actual.adapter.source_set, &common_package_path);
            files.push(KMPOutputFile {
                relative_path: actual_dir.join(format!(
                    "{}{}.kt",
                    module_name, actual.adapter.actual_file_suffix
                )),
                contents: actual.contents,
            });
        });

        platform_adapters
            .iter()
            .filter(|adapter| matches!(adapter.backend, KmpActualBackend::KotlinJvm))
            .for_each(|adapter| {
                let internal_dir =
                    Self::source_set_kotlin_dir(adapter.source_set, &internal_package_path);
                files.push(KMPOutputFile {
                    relative_path: internal_dir.join(format!("{module_name}.kt")),
                    contents: jvm_source.clone(),
                });
            });

        platform_adapters
            .iter()
            .filter(|adapter| matches!(adapter.backend, KmpActualBackend::KotlinJvm))
            .for_each(|adapter| {
                files.push(KMPOutputFile {
                    relative_path: PathBuf::from(format!(
                        "src/{}/c/jni_glue.c",
                        adapter.source_set
                    )),
                    contents: jni_source.clone(),
                });
            });

        KMPOutput { files }
    }

    fn default_platform_adapters() -> Vec<KmpPlatformAdapter> {
        vec![KmpPlatformAdapter::jvm(), KmpPlatformAdapter::android()]
    }

    fn source_set_kotlin_dir(source_set: &str, package_path: &Path) -> PathBuf {
        PathBuf::from(format!("src/{source_set}/kotlin")).join(package_path)
    }

    fn render_surfaces(
        contract: &ir::FfiContract,
        package_name: &str,
        internal_package: &str,
        platform_adapters: &[KmpPlatformAdapter],
    ) -> KmpRender {
        let support = KmpSurfaceSupport::for_contract(contract);
        let common = Self::render_common_surface(contract, package_name, &support);
        let platform_actuals = platform_adapters
            .iter()
            .map(|adapter| KmpPlatformActual {
                adapter: *adapter,
                contents: Self::render_platform_actual(
                    contract,
                    package_name,
                    internal_package,
                    &support,
                    *adapter,
                ),
            })
            .collect();

        KmpRender {
            common,
            platform_actuals,
        }
    }

    fn render_common_surface(
        contract: &ir::FfiContract,
        package_name: &str,
        support: &KmpSurfaceSupport,
    ) -> String {
        let mut common_sections = Vec::new();
        common_sections.push("// Auto-generated by BoltFFI. Do not edit.".to_string());
        common_sections.push(format!("package {package_name}"));

        let mut unsupported = Vec::new();

        contract
            .catalog
            .all_custom_types()
            .filter(|custom| support.custom_types.contains(custom.id.as_str()))
            .map(Self::render_custom_type)
            .for_each(|section| common_sections.push(section));

        contract
            .catalog
            .all_records()
            .filter(|record| support.records.contains(record.id.as_str()))
            .map(Self::render_common_record)
            .for_each(|section| common_sections.push(section));

        contract
            .catalog
            .all_enums()
            .filter(|enumeration| support.enums.contains(enumeration.id.as_str()))
            .map(Self::render_common_enum)
            .for_each(|section| common_sections.push(section));

        contract.functions.iter().for_each(|function| {
            if function_supported(function, contract, support) {
                common_sections.push(Self::render_common_function(function));
            } else {
                unsupported.push(function.id.as_str().to_string());
            }
        });

        if !unsupported.is_empty() {
            common_sections.push(format!(
                "// Unsupported in the initial KMP generator slice: {}",
                unsupported.join(", ")
            ));
        }

        join_kotlin_sections(common_sections)
    }

    fn render_platform_actual(
        contract: &ir::FfiContract,
        package_name: &str,
        internal_package: &str,
        support: &KmpSurfaceSupport,
        adapter: KmpPlatformAdapter,
    ) -> String {
        match adapter.backend {
            KmpActualBackend::KotlinJvm => {
                Self::render_kotlin_jvm_actual(contract, package_name, internal_package, support)
            }
        }
    }

    fn render_kotlin_jvm_actual(
        contract: &ir::FfiContract,
        package_name: &str,
        internal_package: &str,
        support: &KmpSurfaceSupport,
    ) -> String {
        let mut actual_sections = Vec::new();
        actual_sections.push("// Auto-generated by BoltFFI. Do not edit.".to_string());
        actual_sections.push(format!("package {package_name}"));

        contract
            .catalog
            .all_records()
            .filter(|record| support.records.contains(record.id.as_str()))
            .map(|record| {
                Self::render_record_actual_conversions(record, internal_package, contract)
            })
            .for_each(|section| actual_sections.push(section));

        contract
            .functions
            .iter()
            .filter(|function| function_supported(function, contract, support))
            .map(|function| {
                Self::render_kotlin_jvm_function_actual(function, contract, internal_package)
            })
            .for_each(|section| actual_sections.push(section));

        join_kotlin_sections(actual_sections)
    }

    fn render_custom_type(custom: &ir::definitions::CustomTypeDef) -> String {
        format!(
            "typealias {} = {}",
            NamingConvention::class_name(custom.id.as_str()),
            common_type_name(&custom.repr)
        )
    }

    fn render_common_record(record: &ir::definitions::RecordDef) -> String {
        if record.fields.is_empty() {
            return format!(
                "{}object {}",
                kdoc_block(&record.doc),
                NamingConvention::class_name(record.id.as_str())
            );
        }

        let params = record
            .fields
            .iter()
            .map(|field| {
                format!(
                    "val {}: {}",
                    NamingConvention::property_name(field.name.as_str()),
                    common_type_name(&field.type_expr)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");

        format!(
            "{}data class {}({})",
            kdoc_block(&record.doc),
            NamingConvention::class_name(record.id.as_str()),
            params
        )
    }

    fn render_record_actual_conversions(
        record: &ir::definitions::RecordDef,
        internal_package: &str,
        contract: &ir::FfiContract,
    ) -> String {
        let class_name = NamingConvention::class_name(record.id.as_str());
        if record.fields.is_empty() {
            return format!(
                "private fun {class_name}.toBoltFfiJvm(): {internal_package}.{class_name} = {internal_package}.{class_name}\n\nprivate fun {internal_package}.{class_name}.toBoltFfiCommon(): {class_name} = {class_name}"
            );
        }

        let to_jvm_args = record
            .fields
            .iter()
            .map(|field| {
                let name = NamingConvention::property_name(field.name.as_str());
                format!(
                    "{} = {}",
                    name,
                    to_jvm_expr(&field.type_expr, &name, contract, internal_package)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let to_common_args = record
            .fields
            .iter()
            .map(|field| {
                let name = NamingConvention::property_name(field.name.as_str());
                format!(
                    "{} = {}",
                    name,
                    to_common_expr(&field.type_expr, &name, contract)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");

        format!(
            "private fun {class_name}.toBoltFfiJvm(): {internal_package}.{class_name} = {internal_package}.{class_name}({to_jvm_args})\n\nprivate fun {internal_package}.{class_name}.toBoltFfiCommon(): {class_name} = {class_name}({to_common_args})"
        )
    }

    fn render_common_enum(enumeration: &ir::definitions::EnumDef) -> String {
        let ir::definitions::EnumRepr::CStyle { tag_type, variants } = &enumeration.repr else {
            unreachable!("caller filters data enums");
        };
        let value_type = enum_value_type(*tag_type);
        let entries = variants
            .iter()
            .map(|variant| {
                format!(
                    "{}({})",
                    NamingConvention::enum_entry_name(variant.name.as_str()),
                    enum_literal(variant.discriminant, *tag_type)
                )
            })
            .collect::<Vec<_>>()
            .join(",\n    ");
        let class_name = NamingConvention::class_name(enumeration.id.as_str());

        format!(
            "{}enum class {class_name}(val value: {value_type}) {{\n    {entries};\n\n    companion object {{\n        fun fromValue(value: {value_type}): {class_name} = entries.firstOrNull {{ it.value == value }} ?: throw IllegalArgumentException(\"Unknown {class_name} value: $value\")\n    }}\n}}",
            kdoc_block(&enumeration.doc)
        )
    }

    fn render_common_function(function: &ir::definitions::FunctionDef) -> String {
        let function_name = NamingConvention::method_name(function.id.as_str());
        let suspend_prefix = if function.is_async() { "suspend " } else { "" };
        let params = Self::render_common_function_params(function);
        let return_type = return_type_name(&function.returns);
        let return_suffix = return_type
            .as_ref()
            .map(|ty| format!(": {ty}"))
            .unwrap_or_default();

        format!(
            "{}expect {suspend_prefix}fun {function_name}({params}){return_suffix}",
            kdoc_block(&function.doc)
        )
    }

    fn render_kotlin_jvm_function_actual(
        function: &ir::definitions::FunctionDef,
        contract: &ir::FfiContract,
        internal_package: &str,
    ) -> String {
        let function_name = NamingConvention::method_name(function.id.as_str());
        let suspend_prefix = if function.is_async() { "suspend " } else { "" };
        let params = Self::render_common_function_params(function);
        let return_type = return_type_name(&function.returns);
        let return_suffix = return_type
            .as_ref()
            .map(|ty| format!(": {ty}"))
            .unwrap_or_default();

        let args = function
            .params
            .iter()
            .map(|param| {
                let name = NamingConvention::param_name(param.name.as_str());
                to_jvm_expr(&param.type_expr, &name, contract, internal_package)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let delegated = format!("{internal_package}.{function_name}({args})");
        let actual_body = return_type_expr(&function.returns, delegated, contract);

        format!(
            "{}actual {suspend_prefix}fun {function_name}({params}){return_suffix} = {actual_body}",
            kdoc_block(&function.doc)
        )
    }

    fn render_common_function_params(function: &ir::definitions::FunctionDef) -> String {
        function
            .params
            .iter()
            .map(|param| {
                format!(
                    "{}: {}",
                    NamingConvention::param_name(param.name.as_str()),
                    common_type_name(&param.type_expr)
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn render_build_gradle(package_name: &str, min_sdk: u32) -> String {
        format!(
            r#"import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {{
    kotlin("multiplatform") version "2.3.21"
    id("com.android.library") version "8.5.2"
}}

kotlin {{
    jvm {{
        compilerOptions {{
            jvmTarget.set(JvmTarget.JVM_1_8)
        }}
    }}

    androidTarget {{
        compilerOptions {{
            jvmTarget.set(JvmTarget.JVM_1_8)
        }}
    }}

    sourceSets {{
        commonMain.dependencies {{
            implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.11.0")
        }}
    }}
}}

android {{
    namespace = "{package_name}"
    compileSdk = 35

    defaultConfig {{
        minSdk = {min_sdk}
    }}

    compileOptions {{
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }}
}}
"#
        )
    }

    fn render_settings_gradle(module_name: &str) -> String {
        format!(
            r#"pluginManagement {{
    repositories {{
        google()
        mavenCentral()
        gradlePluginPortal()
    }}
}}

dependencyResolutionManagement {{
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {{
        google()
        mavenCentral()
    }}
}}

rootProject.name = "{}-kmp"
"#,
            module_name.to_lowercase()
        )
    }
}

fn join_kotlin_sections(sections: Vec<String>) -> String {
    let mut out = sections
        .into_iter()
        .map(|section| section.trim().to_string())
        .filter(|section| !section.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    out.push('\n');
    out
}

fn kdoc_block(doc: &Option<String>) -> String {
    doc.as_ref()
        .map(|text| {
            let mut result = "/**\n".to_string();
            text.lines()
                .for_each(|line| result.push_str(&format!(" * {line}\n")));
            result.push_str(" */\n");
            result
        })
        .unwrap_or_default()
}

fn supported_records(contract: &ir::FfiContract) -> HashSet<String> {
    let mut supported = HashSet::new();
    loop {
        let before = supported.len();
        contract.catalog.all_records().for_each(|record| {
            if record.fields.iter().all(|field| {
                type_supported_with_sets(
                    &field.type_expr,
                    contract,
                    &supported,
                    &supported_c_style_enums(contract),
                    &supported_custom_types(
                        contract,
                        &supported,
                        &supported_c_style_enums(contract),
                    ),
                )
            }) {
                supported.insert(record.id.as_str().to_string());
            }
        });
        if supported.len() == before {
            break;
        }
    }
    supported
}

fn supported_c_style_enums(contract: &ir::FfiContract) -> HashSet<String> {
    contract
        .catalog
        .all_enums()
        .filter(|enumeration| !enumeration.is_error)
        .filter(|enumeration| matches!(enumeration.repr, ir::definitions::EnumRepr::CStyle { .. }))
        .map(|enumeration| enumeration.id.as_str().to_string())
        .collect()
}

fn supported_custom_types(
    contract: &ir::FfiContract,
    records: &HashSet<String>,
    enums: &HashSet<String>,
) -> HashSet<String> {
    let mut supported = HashSet::new();
    loop {
        let before = supported.len();
        contract.catalog.all_custom_types().for_each(|custom| {
            if type_supported_with_sets(&custom.repr, contract, records, enums, &supported) {
                supported.insert(custom.id.as_str().to_string());
            }
        });
        if supported.len() == before {
            break;
        }
    }
    supported
}

fn type_supported(
    ty: &ir::types::TypeExpr,
    contract: &ir::FfiContract,
    support: &KmpSurfaceSupport,
) -> bool {
    type_supported_with_sets(
        ty,
        contract,
        &support.records,
        &support.enums,
        &support.custom_types,
    )
}

fn type_supported_with_sets(
    ty: &ir::types::TypeExpr,
    contract: &ir::FfiContract,
    records: &HashSet<String>,
    enums: &HashSet<String>,
    custom_types: &HashSet<String>,
) -> bool {
    let _ = contract;
    match ty {
        ir::types::TypeExpr::Void
        | ir::types::TypeExpr::Primitive(_)
        | ir::types::TypeExpr::String
        | ir::types::TypeExpr::Bytes => true,
        ir::types::TypeExpr::Vec(inner) | ir::types::TypeExpr::Option(inner) => {
            type_supported_with_sets(inner, contract, records, enums, custom_types)
        }
        ir::types::TypeExpr::Record(id) => records.contains(id.as_str()),
        ir::types::TypeExpr::Enum(id) => enums.contains(id.as_str()),
        ir::types::TypeExpr::Custom(id) => custom_types.contains(id.as_str()),
        ir::types::TypeExpr::Builtin(_)
        | ir::types::TypeExpr::Callback(_)
        | ir::types::TypeExpr::Handle(_)
        | ir::types::TypeExpr::Result { .. } => false,
    }
}

fn return_supported(
    returns: &ir::definitions::ReturnDef,
    contract: &ir::FfiContract,
    support: &KmpSurfaceSupport,
) -> bool {
    match returns {
        ir::definitions::ReturnDef::Void => true,
        ir::definitions::ReturnDef::Value(ty) => type_supported(ty, contract, support),
        ir::definitions::ReturnDef::Result { .. } => false,
    }
}

fn function_supported(
    function: &ir::definitions::FunctionDef,
    contract: &ir::FfiContract,
    support: &KmpSurfaceSupport,
) -> bool {
    return_supported(&function.returns, contract, support)
        && function
            .params
            .iter()
            .all(|param| type_supported(&param.type_expr, contract, support))
}

fn common_type_name(ty: &ir::types::TypeExpr) -> String {
    match ty {
        ir::types::TypeExpr::Void => "Unit".to_string(),
        ir::types::TypeExpr::Primitive(primitive) => primitive_type_name(*primitive),
        ir::types::TypeExpr::String => "String".to_string(),
        ir::types::TypeExpr::Bytes => "ByteArray".to_string(),
        ir::types::TypeExpr::Vec(inner) => vec_type_name(inner),
        ir::types::TypeExpr::Option(inner) => format!("{}?", common_type_name(inner)),
        ir::types::TypeExpr::Record(id) => NamingConvention::class_name(id.as_str()),
        ir::types::TypeExpr::Enum(id) => NamingConvention::class_name(id.as_str()),
        ir::types::TypeExpr::Custom(id) => NamingConvention::class_name(id.as_str()),
        ir::types::TypeExpr::Builtin(id) => NamingConvention::class_name(id.as_str()),
        ir::types::TypeExpr::Handle(id) => NamingConvention::class_name(id.as_str()),
        ir::types::TypeExpr::Callback(id) => NamingConvention::class_name(id.as_str()),
        ir::types::TypeExpr::Result { ok, err } => {
            format!(
                "BoltFFIResult<{}, {}>",
                common_type_name(ok),
                common_type_name(err)
            )
        }
    }
}

fn vec_type_name(inner: &ir::types::TypeExpr) -> String {
    match inner {
        ir::types::TypeExpr::Primitive(primitive) => match primitive {
            ir::types::PrimitiveType::I32 | ir::types::PrimitiveType::U32 => "IntArray".to_string(),
            ir::types::PrimitiveType::I16 | ir::types::PrimitiveType::U16 => {
                "ShortArray".to_string()
            }
            ir::types::PrimitiveType::I64
            | ir::types::PrimitiveType::U64
            | ir::types::PrimitiveType::ISize
            | ir::types::PrimitiveType::USize => "LongArray".to_string(),
            ir::types::PrimitiveType::F32 => "FloatArray".to_string(),
            ir::types::PrimitiveType::F64 => "DoubleArray".to_string(),
            ir::types::PrimitiveType::U8 | ir::types::PrimitiveType::I8 => "ByteArray".to_string(),
            ir::types::PrimitiveType::Bool => "BooleanArray".to_string(),
        },
        _ => format!("List<{}>", common_type_name(inner)),
    }
}

fn primitive_type_name(primitive: ir::types::PrimitiveType) -> String {
    match primitive {
        ir::types::PrimitiveType::Bool => "Boolean".to_string(),
        ir::types::PrimitiveType::I8 => "Byte".to_string(),
        ir::types::PrimitiveType::U8 => "UByte".to_string(),
        ir::types::PrimitiveType::I16 => "Short".to_string(),
        ir::types::PrimitiveType::U16 => "UShort".to_string(),
        ir::types::PrimitiveType::I32 => "Int".to_string(),
        ir::types::PrimitiveType::U32 => "UInt".to_string(),
        ir::types::PrimitiveType::I64 | ir::types::PrimitiveType::ISize => "Long".to_string(),
        ir::types::PrimitiveType::U64 | ir::types::PrimitiveType::USize => "ULong".to_string(),
        ir::types::PrimitiveType::F32 => "Float".to_string(),
        ir::types::PrimitiveType::F64 => "Double".to_string(),
    }
}

fn enum_value_type(primitive: ir::types::PrimitiveType) -> String {
    match primitive {
        ir::types::PrimitiveType::I8 | ir::types::PrimitiveType::U8 => "Byte".to_string(),
        ir::types::PrimitiveType::I16 | ir::types::PrimitiveType::U16 => "Short".to_string(),
        ir::types::PrimitiveType::I32 | ir::types::PrimitiveType::U32 => "Int".to_string(),
        ir::types::PrimitiveType::I64
        | ir::types::PrimitiveType::U64
        | ir::types::PrimitiveType::ISize
        | ir::types::PrimitiveType::USize => "Long".to_string(),
        _ => primitive_type_name(primitive),
    }
}

fn enum_literal(value: i128, primitive: ir::types::PrimitiveType) -> String {
    kotlin_integer_literal(value, &enum_value_type(primitive))
}

fn kotlin_integer_literal(value: i128, kotlin_type: &str) -> String {
    match kotlin_type {
        "Byte" => format!("({value}L).toByte()"),
        "Short" => format!("({value}L).toShort()"),
        "Int" => {
            if i32::try_from(value).is_ok() {
                value.to_string()
            } else {
                format!("({value}L).toInt()")
            }
        }
        "Long" => {
            if i64::try_from(value).is_ok() {
                format!("{value}L")
            } else {
                format!("({value}uL).toLong()")
            }
        }
        _ => value.to_string(),
    }
}

fn return_type_name(returns: &ir::definitions::ReturnDef) -> Option<String> {
    match returns {
        ir::definitions::ReturnDef::Void => None,
        ir::definitions::ReturnDef::Value(ty) => Some(common_type_name(ty)),
        ir::definitions::ReturnDef::Result { ok, .. } => Some(common_type_name(ok)),
    }
}

fn to_jvm_expr(
    ty: &ir::types::TypeExpr,
    expr: &str,
    contract: &ir::FfiContract,
    internal_package: &str,
) -> String {
    match ty {
        ir::types::TypeExpr::Primitive(_) => expr.to_string(),
        ir::types::TypeExpr::Record(_) => format!("{expr}.toBoltFfiJvm()"),
        ir::types::TypeExpr::Enum(id) => {
            let class_name = NamingConvention::class_name(id.as_str());
            format!("{internal_package}.{class_name}.fromValue({expr}.value)")
        }
        ir::types::TypeExpr::Vec(inner) => to_jvm_vec_expr(inner, expr, contract, internal_package),
        ir::types::TypeExpr::Option(inner) => {
            format!(
                "{expr}?.let {{ {} }}",
                to_jvm_expr(inner, "it", contract, internal_package)
            )
        }
        ir::types::TypeExpr::Custom(id) => contract
            .catalog
            .resolve_custom(id)
            .map(|custom| to_jvm_expr(&custom.repr, expr, contract, internal_package))
            .unwrap_or_else(|| expr.to_string()),
        _ => expr.to_string(),
    }
}

fn to_jvm_vec_expr(
    ty: &ir::types::TypeExpr,
    expr: &str,
    contract: &ir::FfiContract,
    internal_package: &str,
) -> String {
    match ty {
        ir::types::TypeExpr::Primitive(_) => expr.to_string(),
        _ => format!(
            "{expr}.map {{ {} }}",
            to_jvm_expr(ty, "it", contract, internal_package)
        ),
    }
}

fn to_common_expr(ty: &ir::types::TypeExpr, expr: &str, contract: &ir::FfiContract) -> String {
    match ty {
        ir::types::TypeExpr::Primitive(_) => expr.to_string(),
        ir::types::TypeExpr::Record(_) => format!("{expr}.toBoltFfiCommon()"),
        ir::types::TypeExpr::Enum(id) => {
            let class_name = NamingConvention::class_name(id.as_str());
            format!("{class_name}.fromValue({expr}.value)")
        }
        ir::types::TypeExpr::Vec(inner) => to_common_vec_expr(inner, expr, contract),
        ir::types::TypeExpr::Option(inner) => {
            format!(
                "{expr}?.let {{ {} }}",
                to_common_expr(inner, "it", contract)
            )
        }
        ir::types::TypeExpr::Custom(id) => contract
            .catalog
            .resolve_custom(id)
            .map(|custom| to_common_expr(&custom.repr, expr, contract))
            .unwrap_or_else(|| expr.to_string()),
        _ => expr.to_string(),
    }
}

fn to_common_vec_expr(ty: &ir::types::TypeExpr, expr: &str, contract: &ir::FfiContract) -> String {
    match ty {
        ir::types::TypeExpr::Primitive(_) => expr.to_string(),
        _ => format!("{expr}.map {{ {} }}", to_common_expr(ty, "it", contract)),
    }
}

fn return_type_expr(
    returns: &ir::definitions::ReturnDef,
    delegated: String,
    contract: &ir::FfiContract,
) -> String {
    match returns {
        ir::definitions::ReturnDef::Void => delegated,
        ir::definitions::ReturnDef::Value(ty) => to_common_expr(ty, &delegated, contract),
        ir::definitions::ReturnDef::Result { ok, .. } => to_common_expr(ok, &delegated, contract),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_record() -> ir::definitions::RecordDef {
        ir::definitions::RecordDef {
            id: "Empty".into(),
            is_repr_c: false,
            is_error: false,
            fields: Vec::new(),
            constructors: Vec::new(),
            methods: Vec::new(),
            doc: None,
            deprecated: None,
        }
    }

    fn empty_contract() -> ir::FfiContract {
        ir::FfiContract {
            package: ir::PackageInfo {
                name: "demo".to_string(),
                version: None,
            },
            catalog: ir::TypeCatalog::new(),
            functions: Vec::new(),
        }
    }

    #[test]
    fn empty_records_render_as_objects() {
        let record = empty_record();

        let common = KMPEmitter::render_common_record(&record);
        let actual = KMPEmitter::render_record_actual_conversions(
            &record,
            "com.example.demo.jvm",
            &empty_contract(),
        );

        assert_eq!(common, "object Empty");
        assert!(actual.contains("= com.example.demo.jvm.Empty"));
        assert!(actual.contains("= Empty"));
        assert!(!common.contains("data class Empty()"));
        assert!(!actual.contains("Empty()"));
    }

    #[test]
    fn default_platform_adapters_are_jvm_family_actuals() {
        let adapters = KMPEmitter::default_platform_adapters();

        assert_eq!(
            adapters,
            vec![KmpPlatformAdapter::jvm(), KmpPlatformAdapter::android()]
        );
        assert!(
            adapters
                .iter()
                .all(|adapter| matches!(adapter.backend, KmpActualBackend::KotlinJvm))
        );
    }

    #[test]
    fn surfaces_render_common_once_and_platform_actuals_separately() {
        let adapters = KMPEmitter::default_platform_adapters();
        let rendered = KMPEmitter::render_surfaces(
            &empty_contract(),
            "com.example.demo",
            "com.example.demo.jvm",
            &adapters,
        );

        assert!(rendered.common.contains("package com.example.demo"));
        assert_eq!(rendered.platform_actuals.len(), 2);
        assert_eq!(rendered.platform_actuals[0].adapter.source_set, "jvmMain");
        assert_eq!(
            rendered.platform_actuals[1].adapter.source_set,
            "androidMain"
        );
        assert!(
            rendered
                .platform_actuals
                .iter()
                .all(|actual| actual.contents.contains("package com.example.demo"))
        );
    }

    #[test]
    fn unsigned_enum_discriminants_render_as_valid_signed_kotlin_literals() {
        assert_eq!(
            enum_literal(u32::MAX as i128, ir::types::PrimitiveType::U32),
            "(4294967295L).toInt()"
        );
        assert_eq!(
            enum_literal(u64::MAX as i128, ir::types::PrimitiveType::U64),
            "(18446744073709551615uL).toLong()"
        );
    }
}

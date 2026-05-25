use askama::Template;

#[derive(Template)]
#[template(path = "render_dart/prelude.txt", escape = "none")]
pub struct PreludeTemplate {}

#[derive(Template)]
#[template(path = "render_dart/custom_types.txt", escape = "none")]
pub struct CustomTypesTemplate<'a> {
    pub custom_types: &'a [super::DartCustomType],
}

#[derive(Template)]
#[template(path = "render_dart/native_functions.txt", escape = "none")]
pub struct NativeFunctionsTemplate<'a> {
    pub cfuncs: &'a [super::DartNativeFunction],
}

#[derive(Template)]
#[template(path = "render_dart/record.txt", escape = "none")]
pub struct RecordTemplate<'a> {
    pub record: &'a super::DartRecord,
}

#[derive(Template)]
#[template(path = "render_dart/hook.build.dart.txt", escape = "none")]
pub struct BuildHookTemplate<'a> {
    pub artifact_name: &'a str,
}

#[derive(Template)]
#[template(path = "render_dart/pubspec.yaml.txt", escape = "none")]
pub struct PubspecTemplate<'a> {
    pub artifact_name: &'a str,
    pub description: Option<&'a str>,
    pub version: Option<&'a str>,
    pub repository: Option<&'a str>,
}

#[derive(Template)]
#[template(path = "render_dart/enum.txt", escape = "none")]
pub struct EnhancedEnumTemplate<'a> {
    pub dart_enum: &'a super::DartEnum,
}

#[derive(Template)]
#[template(path = "render_dart/sealed_class_enum.txt", escape = "none")]
pub struct SealedClassEnumTemplate<'a> {
    pub dart_enum: &'a super::DartEnum,
}

#[derive(Template)]
#[template(path = "render_dart/callback.txt", escape = "none")]
pub struct CallbackTemplate<'a> {
    pub cb: &'a super::DartCallback,
}

#[cfg(test)]
mod tests {
    use boltffi_ffi_rules::callable::ExecutionKind;

    use crate::{
        ir::{
            self, CallbackId, CallbackKind, CallbackMethodDef, CallbackTraitDef, FfiContract,
            MethodId, PackageInfo, ParamDef, ParamName, ParamPassing, PrimitiveType, ReturnDef,
            TypeExpr,
        },
        render::dart::{DartLibrary, DartLowerer},
    };

    use super::*;

    fn empty_contract() -> FfiContract {
        FfiContract {
            package: PackageInfo {
                name: "test".to_string(),
                version: None,
            },
            functions: vec![],
            catalog: Default::default(),
        }
    }

    fn lower(ffi: &FfiContract) -> DartLibrary {
        let abi = ir::Lowerer::new(ffi).to_abi_contract();

        DartLowerer::new(ffi, &abi, "test").library()
    }

    fn generic_callback_def(kind: ExecutionKind) -> CallbackTraitDef {
        CallbackTraitDef {
            id: CallbackId::new("ICallback"),
            methods: vec![
                CallbackMethodDef {
                    execution_kind: kind,
                    id: MethodId::new("map_u32"),
                    params: vec![ParamDef {
                        name: ParamName::new("n"),
                        type_expr: TypeExpr::Primitive(PrimitiveType::U32),
                        passing: ParamPassing::Value,
                        doc: None,
                    }],
                    returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::U32)),
                    doc: None,
                },
                CallbackMethodDef {
                    execution_kind: kind,
                    id: MethodId::new("map_string_ref"),
                    params: vec![ParamDef {
                        name: ParamName::new("s"),
                        type_expr: TypeExpr::String,
                        passing: ParamPassing::Ref,
                        doc: None,
                    }],
                    returns: ReturnDef::Value(TypeExpr::String),
                    doc: None,
                },
                CallbackMethodDef {
                    execution_kind: kind,
                    id: MethodId::new("map_string"),
                    params: vec![ParamDef {
                        name: ParamName::new("s"),
                        type_expr: TypeExpr::String,
                        passing: ParamPassing::Value,
                        doc: None,
                    }],
                    returns: ReturnDef::Value(TypeExpr::String),
                    doc: None,
                },
                CallbackMethodDef {
                    execution_kind: kind,
                    id: MethodId::new("process_bytes"),
                    params: vec![ParamDef {
                        name: ParamName::new("bytes"),
                        type_expr: TypeExpr::Bytes,
                        passing: ParamPassing::Value,
                        doc: None,
                    }],
                    returns: ReturnDef::Void,
                    doc: None,
                },
            ],
            kind: CallbackKind::Trait,
            doc: None,
        }
    }

    #[test]
    pub fn snapshot_sync_callback() {
        let mut ffi = empty_contract();
        ffi.catalog
            .insert_callback(generic_callback_def(ExecutionKind::Sync));
        let library = lower(&ffi);

        let template = CallbackTemplate {
            cb: &library.callbacks[0],
        };

        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    pub fn snapshot_async_callback() {
        let mut ffi = empty_contract();
        ffi.catalog
            .insert_callback(generic_callback_def(ExecutionKind::Async));
        let library = lower(&ffi);

        let template = CallbackTemplate {
            cb: &library.callbacks[0],
        };

        insta::assert_snapshot!(template.render().unwrap());
    }
}

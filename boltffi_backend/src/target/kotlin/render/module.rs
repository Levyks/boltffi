use askama::Template as AskamaTemplate;
use boltffi_binding::{BuiltinType, DeclarationRef, Native};

use crate::{
    bridge::jni::JniBridgeContract,
    core::{
        Diagnostic, Error, FilePath, GeneratedFile, GeneratedOutput, RenderContext,
        RenderedDeclaration, Result,
    },
    target::kotlin::{
        KotlinHost, KotlinPackage,
        render::{
            closure::Closures,
            native::{NativeFunction, NativeMethods},
        },
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/module.kt", escape = "none")]
struct ModuleTemplate {
    package: KotlinPackage,
    runtime: String,
    closures: String,
    native_functions: Vec<NativeFunction>,
    declarations: String,
    async_runtime: bool,
}

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/runtime.kt", escape = "none")]
struct RuntimeTemplate;

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/runtime/result.kt", escape = "none")]
struct ResultRuntimeTemplate;

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/runtime/builtin.kt", escape = "none")]
struct BuiltinRuntimeTemplate;

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/runtime/async.kt", escape = "none")]
struct AsyncRuntimeTemplate;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RuntimeFeatures {
    asynchronous: bool,
    builtin: bool,
    result: bool,
}

pub struct Module<'host, 'bridge, 'decl> {
    host: &'host KotlinHost,
    bridge: &'bridge JniBridgeContract,
    context: &'decl RenderContext<'decl, Native>,
    declarations: Vec<RenderedDeclaration<'decl, Native>>,
}

impl<'host, 'bridge, 'decl> Module<'host, 'bridge, 'decl> {
    pub fn new(
        host: &'host KotlinHost,
        bridge: &'bridge JniBridgeContract,
        context: &'decl RenderContext<'decl, Native>,
        declarations: Vec<RenderedDeclaration<'decl, Native>>,
    ) -> Self {
        Self {
            host,
            bridge,
            context,
            declarations,
        }
    }

    pub fn render(self) -> Result<GeneratedOutput> {
        let diagnostics = self.diagnostics();
        let native_functions = self.native_functions()?;
        let closures = self.closures()?;
        let declarations = self.declarations();
        let features = RuntimeFeatures::from_declarations(&self.declarations);
        let contents = ModuleTemplate {
            package: self.host.package().clone(),
            runtime: Runtime::new(features).render()?,
            closures,
            native_functions,
            declarations,
            async_runtime: features.asynchronous,
        }
        .render()?;
        Ok(GeneratedOutput::new(
            vec![GeneratedFile::new(
                FilePath::new(self.host.file().path(self.host.package()))?,
                contents,
            )],
            diagnostics,
        ))
    }

    fn native_functions(&self) -> Result<Vec<NativeFunction>> {
        let methods = NativeMethods::new(self.bridge);
        let functions = self
            .declarations
            .iter()
            .filter(|declaration| !declaration.emitted().primary_chunk().is_empty())
            .map(|declaration| match declaration.declaration() {
                DeclarationRef::Function(function) => methods.function(function),
                DeclarationRef::Class(class) => methods.class(class),
                DeclarationRef::Callback(callback) => methods.callback(callback),
                _ => Ok(Vec::new()),
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .chain(methods.callback_handle_lifecycle()?)
            .chain(methods.callback_completions()?)
            .chain(methods.success_out_writers()?)
            .collect::<Vec<_>>();
        Self::unique_native_functions(functions)
    }

    fn closures(&self) -> Result<String> {
        Ok(
            Closures::from_declarations(&self.declarations, self.bridge, self.context)?
                .render()?
                .into_iter()
                .collect::<Vec<_>>()
                .join("\n\n"),
        )
    }

    fn unique_native_functions(functions: Vec<NativeFunction>) -> Result<Vec<NativeFunction>> {
        functions
            .into_iter()
            .try_fold(Vec::new(), |mut unique, function| {
                if unique
                    .iter()
                    .any(|existing: &NativeFunction| existing.name() == function.name())
                {
                    Err(Error::KotlinNameCollision {
                        scope: "Native".to_owned(),
                        name: function.name().to_string(),
                    })
                } else {
                    unique.push(function);
                    Ok(unique)
                }
            })
    }

    fn functions(&self) -> Vec<String> {
        self.primary_chunks(|declaration| {
            matches!(declaration.declaration(), DeclarationRef::Function(_))
        })
    }

    fn records(&self) -> Vec<String> {
        self.primary_chunks(|declaration| {
            matches!(declaration.declaration(), DeclarationRef::Record(_))
        })
    }

    fn enumerations(&self) -> Vec<String> {
        self.primary_chunks(|declaration| {
            matches!(declaration.declaration(), DeclarationRef::Enum(_))
        })
    }

    fn classes(&self) -> Vec<String> {
        self.primary_chunks(|declaration| {
            matches!(declaration.declaration(), DeclarationRef::Class(_))
        })
    }

    fn callbacks(&self) -> Vec<String> {
        self.primary_chunks(|declaration| {
            matches!(declaration.declaration(), DeclarationRef::Callback(_))
        })
    }

    fn declarations(&self) -> String {
        [
            self.records(),
            self.enumerations(),
            self.callbacks(),
            self.classes(),
            self.functions(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n\n")
    }

    fn primary_chunks(
        &self,
        include: impl Fn(&RenderedDeclaration<'decl, Native>) -> bool,
    ) -> Vec<String> {
        self.declarations
            .iter()
            .filter_map(|declaration| {
                let chunk = declaration.emitted().primary_chunk();
                (include(declaration) && !chunk.is_empty())
                    .then(|| chunk.as_str().trim_end().to_owned())
            })
            .collect()
    }

    fn diagnostics(&self) -> Vec<Diagnostic> {
        self.declarations
            .iter()
            .flat_map(|declaration| declaration.emitted().diagnostics().iter().cloned())
            .collect()
    }
}

struct Runtime {
    features: RuntimeFeatures,
}

impl Runtime {
    fn new(features: RuntimeFeatures) -> Self {
        Self { features }
    }

    fn render(self) -> Result<String> {
        let mut blocks = vec![RuntimeTemplate.render()?];
        if self.features.asynchronous {
            blocks.push(AsyncRuntimeTemplate.render()?);
        }
        if self.features.builtin {
            blocks.push(BuiltinRuntimeTemplate.render()?);
        }
        if self.features.result {
            blocks.push(ResultRuntimeTemplate.render()?);
        }
        Ok(blocks.join("\n\n"))
    }
}

impl RuntimeFeatures {
    fn from_declarations(declarations: &[RenderedDeclaration<'_, Native>]) -> Self {
        Self {
            asynchronous: declarations
                .iter()
                .any(|declaration| declaration.declaration().uses_async_execution()),
            builtin: declarations
                .iter()
                .any(|declaration| Self::declaration_uses_builtin(declaration.declaration())),
            result: declarations
                .iter()
                .any(|declaration| declaration.declaration().uses_result_codec()),
        }
    }

    fn declaration_uses_builtin(declaration: DeclarationRef<'_, Native>) -> bool {
        [
            BuiltinType::Duration,
            BuiltinType::SystemTime,
            BuiltinType::Uuid,
            BuiltinType::Url,
        ]
        .into_iter()
        .any(|kind| declaration.uses_builtin_codec(kind))
    }
}

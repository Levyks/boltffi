use askama::Template as AskamaTemplate;
use std::collections::BTreeSet;

use boltffi_binding::{
    BuiltinType, ClassDecl, ConstantValueDecl, DeclarationRef, EnumDecl, EnumId, ErrorChannel,
    ExportedCallable, ExportedMethodDecl, FunctionDecl, InitializerDecl, Native, NativeSymbol,
    RecordDecl, RecordId, TypeRef,
};

use crate::{
    bridge::jni::JniBridgeContract,
    core::{
        Diagnostic, Error, FilePath, GeneratedFile, GeneratedOutput, RenderContext,
        RenderedDeclaration, Result,
    },
    target::kotlin::{
        KotlinHost, KotlinPackage, NativeLibraries,
        render::{
            closure::Closures,
            enumeration::Enumeration,
            native::{NativeFunction, NativeMethods},
            record::Record,
        },
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/module.kt", escape = "none")]
struct ModuleTemplate {
    package: KotlinPackage,
    native_libraries: NativeLibraries,
    runtime: String,
    closures: String,
    native_functions: Vec<NativeFunction>,
    declarations: String,
    async_runtime: bool,
    stream_runtime: bool,
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

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/runtime/stream.kt", escape = "none")]
struct StreamRuntimeTemplate;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RuntimeFeatures {
    asynchronous: bool,
    streaming: bool,
    builtin: bool,
    result: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ErrorTypes {
    records: BTreeSet<RecordId>,
    enumerations: BTreeSet<EnumId>,
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
        let error_types = ErrorTypes::from_declarations(&self.declarations);
        let declarations = self.declarations(&error_types)?;
        let features = RuntimeFeatures::from_declarations(&self.declarations);
        let contents = ModuleTemplate {
            package: self.host.package().clone(),
            native_libraries: self.host.native_libraries().clone(),
            runtime: Runtime::new(features).render()?,
            closures,
            native_functions,
            declarations,
            async_runtime: features.asynchronous,
            stream_runtime: features.streaming,
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
                DeclarationRef::Record(record) => methods.record(record),
                DeclarationRef::Enum(enumeration) => methods.enumeration(enumeration),
                DeclarationRef::Class(class) => methods.class(class),
                DeclarationRef::Callback(callback) => methods.callback(callback),
                DeclarationRef::Stream(stream) => methods.stream(stream),
                DeclarationRef::Constant(constant) => methods.constant(constant),
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

    fn functions(&self) -> Result<Vec<String>> {
        self.primary_chunks(None, |declaration| {
            matches!(declaration.declaration(), DeclarationRef::Function(_))
        })
    }

    fn records(&self, error_types: &ErrorTypes) -> Result<Vec<String>> {
        self.primary_chunks(Some(error_types), |declaration| {
            matches!(declaration.declaration(), DeclarationRef::Record(_))
        })
    }

    fn enumerations(&self, error_types: &ErrorTypes) -> Result<Vec<String>> {
        self.primary_chunks(Some(error_types), |declaration| {
            matches!(declaration.declaration(), DeclarationRef::Enum(_))
        })
    }

    fn classes(&self) -> Result<Vec<String>> {
        self.primary_chunks(None, |declaration| {
            matches!(declaration.declaration(), DeclarationRef::Class(_))
        })
    }

    fn callbacks(&self) -> Result<Vec<String>> {
        self.primary_chunks(None, |declaration| {
            matches!(declaration.declaration(), DeclarationRef::Callback(_))
        })
    }

    fn streams(&self) -> Result<Vec<String>> {
        self.primary_chunks(None, |declaration| {
            matches!(declaration.declaration(), DeclarationRef::Stream(_))
        })
    }

    fn constants(&self) -> Result<Vec<String>> {
        self.primary_chunks(None, |declaration| {
            matches!(declaration.declaration(), DeclarationRef::Constant(_))
        })
    }

    fn custom_types(&self) -> Result<Vec<String>> {
        self.primary_chunks(None, |declaration| {
            matches!(declaration.declaration(), DeclarationRef::CustomType(_))
        })
    }

    fn declarations(&self, error_types: &ErrorTypes) -> Result<String> {
        Ok([
            self.custom_types()?,
            self.records(error_types)?,
            self.enumerations(error_types)?,
            self.callbacks()?,
            self.classes()?,
            self.streams()?,
            self.constants()?,
            self.functions()?,
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n\n"))
    }

    fn primary_chunks(
        &self,
        error_types: Option<&ErrorTypes>,
        include: impl Fn(&RenderedDeclaration<'decl, Native>) -> bool,
    ) -> Result<Vec<String>> {
        self.declarations
            .iter()
            .filter(|declaration| {
                let chunk = declaration.emitted().primary_chunk();
                include(declaration) && !chunk.is_empty()
            })
            .map(|declaration| self.primary_chunk(declaration, error_types))
            .collect()
    }

    fn primary_chunk(
        &self,
        declaration: &RenderedDeclaration<'decl, Native>,
        error_types: Option<&ErrorTypes>,
    ) -> Result<String> {
        match declaration.declaration() {
            DeclarationRef::Record(record)
                if error_types.is_some_and(|types| types.contains_record(record.id())) =>
            {
                Ok(
                    Record::from_declaration_as_error(record, self.bridge, self.context)?
                        .render()?
                        .primary_chunk()
                        .as_str()
                        .trim_end()
                        .to_owned(),
                )
            }
            DeclarationRef::Enum(enumeration)
                if error_types.is_some_and(|types| types.contains_enum(enumeration.id())) =>
            {
                Ok(Enumeration::from_declaration_as_error(
                    enumeration,
                    self.bridge,
                    self.context,
                    Some(self.host.package()),
                )?
                .render()?
                .primary_chunk()
                .as_str()
                .trim_end()
                .to_owned())
            }
            _ => Ok(declaration
                .emitted()
                .primary_chunk()
                .as_str()
                .trim_end()
                .to_owned()),
        }
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
        if self.features.streaming {
            blocks.push(StreamRuntimeTemplate.render()?);
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
            asynchronous: declarations.iter().any(|declaration| {
                declaration.declaration().uses_async_execution()
                    || matches!(declaration.declaration(), DeclarationRef::Stream(_))
            }),
            streaming: declarations
                .iter()
                .any(|declaration| matches!(declaration.declaration(), DeclarationRef::Stream(_))),
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

impl ErrorTypes {
    fn from_declarations(declarations: &[RenderedDeclaration<'_, Native>]) -> Self {
        declarations
            .iter()
            .map(RenderedDeclaration::declaration)
            .fold(Self::default(), |mut types, declaration| {
                types.insert_declaration(declaration);
                types
            })
    }

    fn contains_record(&self, id: RecordId) -> bool {
        self.records.contains(&id)
    }

    fn contains_enum(&self, id: EnumId) -> bool {
        self.enumerations.contains(&id)
    }

    fn insert_declaration(&mut self, declaration: DeclarationRef<'_, Native>) {
        match declaration {
            DeclarationRef::Function(function) => self.insert_function(function),
            DeclarationRef::Record(record) => self.insert_record(record),
            DeclarationRef::Enum(enumeration) => self.insert_enum(enumeration),
            DeclarationRef::Class(class) => self.insert_class(class),
            DeclarationRef::Constant(constant) => {
                if let ConstantValueDecl::Accessor { callable, .. } = constant.value() {
                    self.insert_callable(callable);
                }
            }
            DeclarationRef::Callback(callback) => {
                if let Some(protocol) = callback.local_protocol() {
                    protocol
                        .methods()
                        .iter()
                        .for_each(|method| self.insert_callable(method.callable()));
                }
            }
            DeclarationRef::Stream(_) | DeclarationRef::CustomType(_) => {}
        }
    }

    fn insert_function(&mut self, function: &FunctionDecl<Native>) {
        self.insert_callable(function.callable());
    }

    fn insert_record(&mut self, record: &RecordDecl<Native>) {
        match record {
            RecordDecl::Direct(record) => {
                self.insert_associated(record.initializers(), record.methods())
            }
            RecordDecl::Encoded(record) => {
                self.insert_associated(record.initializers(), record.methods())
            }
            _ => {}
        }
    }

    fn insert_enum(&mut self, enumeration: &EnumDecl<Native>) {
        match enumeration {
            EnumDecl::CStyle(enumeration) => {
                self.insert_associated(enumeration.initializers(), enumeration.methods())
            }
            EnumDecl::Data(enumeration) => {
                self.insert_associated(enumeration.initializers(), enumeration.methods())
            }
            _ => {}
        }
    }

    fn insert_class(&mut self, class: &ClassDecl<Native>) {
        self.insert_associated(class.initializers(), class.methods());
    }

    fn insert_associated(
        &mut self,
        initializers: &[InitializerDecl<Native>],
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
    ) {
        initializers
            .iter()
            .for_each(|initializer| self.insert_callable(initializer.callable()));
        methods
            .iter()
            .for_each(|method| self.insert_callable(method.callable()));
    }

    fn insert_callable(&mut self, callable: &ExportedCallable<Native>) {
        if let ErrorChannel::Encoded { ty, .. } = callable.error().channel() {
            self.insert_type(ty);
        }
    }

    fn insert_type(&mut self, ty: &TypeRef) {
        match ty {
            TypeRef::Record(record) => {
                self.records.insert(*record);
            }
            TypeRef::Enum(enumeration) => {
                self.enumerations.insert(*enumeration);
            }
            _ => {}
        }
    }
}

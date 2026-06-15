use boltffi_ast::{ClassDef, SourceContract, StreamDef};
use boltffi_binding::{Native, Wasm32};
use proc_macro2::TokenStream;
use quote::quote;

use crate::experimental::{
    error::Error,
    expansion::Expansion,
    surface::RenderSurface,
    wrapper::{self, Render},
};

/// A crate-level wrapper expander.
///
/// The expander owns target selection for an already scanned source contract.
/// Each method accepts the matching lowered expansion, so native and wasm
/// bindings cannot be crossed accidentally.
pub struct Expander<'lowered> {
    source: &'lowered SourceContract,
}

struct SurfaceExpander<'expansion, 'lowered, S: RenderSurface> {
    source: &'lowered SourceContract,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'lowered> Expander<'lowered> {
    /// Creates an expander over the scanned source contract.
    pub const fn new(source: &'lowered SourceContract) -> Self {
        Self { source }
    }

    /// Expands wrappers for the native surface.
    pub fn native<'expansion>(
        &self,
        expansion: &'expansion Expansion<'lowered, Native>,
    ) -> Result<TokenStream, Error> {
        SurfaceExpander::new(self.source, expansion).expand()
    }

    /// Expands wrappers for the wasm32 surface.
    pub fn wasm32<'expansion>(
        &self,
        expansion: &'expansion Expansion<'lowered, Wasm32>,
    ) -> Result<TokenStream, Error> {
        SurfaceExpander::new(self.source, expansion).expand()
    }

    /// Expands wrappers for native and wasm32 in one token stream.
    pub fn all<'native, 'wasm32>(
        &self,
        native: &'native Expansion<'lowered, Native>,
        wasm32: &'wasm32 Expansion<'lowered, Wasm32>,
    ) -> Result<TokenStream, Error> {
        let native = self.native(native)?;
        let wasm32 = self.wasm32(wasm32)?;

        Ok(quote! {
            #native
            #wasm32
        })
    }
}

impl<'expansion, 'lowered, S> SurfaceExpander<'expansion, 'lowered, S>
where
    S: RenderSurface,
    wrapper::callback::Renderer:
        Render<S, wrapper::callback::Trait<'expansion, 'lowered, S>, Output = TokenStream>,
    wrapper::handle::Carrier: Render<
            S,
            wrapper::handle::CarrierInput<S::HandleCarrier>,
            Output = wrapper::handle::CarrierTokens,
        >,
    wrapper::param::direct::Record:
        Render<S, wrapper::param::direct::RecordInput, Output = wrapper::param::Tokens>,
    for<'source_type> wrapper::param::direct::Renderer:
        Render<S, wrapper::param::direct::Input<'source_type>, Output = wrapper::param::Tokens>,
    wrapper::arguments::SyncRenderer: Render<
            S,
            wrapper::arguments::Input<'expansion, 'lowered, S>,
            Output = wrapper::arguments::Tokens,
        >,
    wrapper::returns::Failure:
        Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
    wrapper::returns::Renderer: Render<
            S,
            wrapper::returns::Input<'expansion, 'lowered, S>,
            Output = wrapper::returns::Tokens,
        >,
    wrapper::async_call::Renderer:
        Render<S, wrapper::async_call::Input<'expansion, 'lowered, S>, Output = TokenStream>,
    wrapper::param::encoded::Renderer: Render<
            S,
            wrapper::param::encoded::Input<'expansion, 'lowered, S>,
            Output = wrapper::param::Tokens,
        >,
    wrapper::returns::encoded::Renderer:
        Render<S, wrapper::returns::encoded::Empty<S>, Output = wrapper::returns::encoded::Tokens>,
    for<'codec> wrapper::returns::encoded::Renderer: Render<
            S,
            wrapper::returns::encoded::Input<'expansion, 'codec, 'lowered, S>,
            Output = wrapper::returns::encoded::Tokens,
        >,
{
    const fn new(
        source: &'lowered SourceContract,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self { source, expansion }
    }

    fn expand(self) -> Result<TokenStream, Error> {
        let callbacks = self.callbacks()?;
        let records = self.records()?;
        let enumerations = self.enumerations()?;
        let classes = self.classes()?;
        let streams = self.streams()?;
        let constants = self.constants()?;
        let functions = self.functions()?;

        Ok(quote! {
            #(#callbacks)*
            #(#records)*
            #(#enumerations)*
            #(#classes)*
            #(#streams)*
            #(#constants)*
            #(#functions)*
        })
    }

    fn callbacks(&self) -> Result<Vec<TokenStream>, Error> {
        self.source
            .traits
            .iter()
            .map(|source| {
                let callback = wrapper::callback::Trait::new(
                    self.expansion.callback_trait(source)?,
                    self.expansion,
                );
                <wrapper::callback::Renderer as Render<S, _>>::render(
                    wrapper::callback::Renderer,
                    callback,
                )
            })
            .collect()
    }

    fn records(&self) -> Result<Vec<TokenStream>, Error> {
        self.source
            .records
            .iter()
            .map(|source| {
                wrapper::record::Renderer::new(self.expansion.record(source)?, self.expansion)
                    .render()
            })
            .collect()
    }

    fn enumerations(&self) -> Result<Vec<TokenStream>, Error> {
        self.source
            .enums
            .iter()
            .map(|source| {
                wrapper::enumeration::Renderer::new(
                    self.expansion.enumeration(source)?,
                    self.expansion,
                )
                .render()
            })
            .collect()
    }

    fn classes(&self) -> Result<Vec<TokenStream>, Error> {
        self.source
            .classes
            .iter()
            .map(|source| {
                wrapper::class::Renderer::new(self.expansion.class(source)?, self.expansion)
                    .render()
            })
            .collect()
    }

    fn streams(&self) -> Result<Vec<TokenStream>, Error> {
        self.source
            .streams
            .iter()
            .map(|source| {
                let owner = self.stream_owner(source)?;
                let stream = self.expansion.stream(source)?;
                match owner {
                    Some(owner) => wrapper::stream::Renderer::new(
                        stream,
                        self.expansion.class(owner)?,
                        self.expansion,
                    )
                    .render(),
                    None => wrapper::stream::Renderer::function(stream, self.expansion).render(),
                }
            })
            .collect()
    }

    fn constants(&self) -> Result<Vec<TokenStream>, Error> {
        self.source
            .constants
            .iter()
            .map(|source| {
                wrapper::constant::Renderer::new(self.expansion.constant(source)?, self.expansion)
                    .render()
            })
            .collect()
    }

    fn functions(&self) -> Result<Vec<TokenStream>, Error> {
        self.source
            .functions
            .iter()
            .map(|source| {
                wrapper::function::Renderer::new(self.expansion.function(source)?, self.expansion)
                    .render()
            })
            .collect()
    }

    fn stream_owner(&self, stream: &StreamDef) -> Result<Option<&'lowered ClassDef>, Error> {
        stream
            .owner
            .as_ref()
            .map(|owner| {
                self.source
                    .classes
                    .iter()
                    .find(|class| &class.id == owner)
                    .ok_or(Error::SourceSyntaxMismatch("stream owner class is missing"))
            })
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path as FsPath, PathBuf};
    use std::process::Command;

    use boltffi_ast::{
        CanonicalName, ClassDef, ConstExpr, ConstantDef, ConstantId, EnumDef, EnumId, FieldDef,
        FunctionDef, FunctionId, Literal, MethodDef, MethodId, PackageInfo, ParameterDef,
        Primitive, Receiver, RecordDef, ReprAttr, ReprItem, ReturnDef, SourceContract, SourceName,
        StreamDef, StreamId, TraitDef, TraitId, TypeExpr, VariantDef,
    };
    use boltffi_binding::{Native, Wasm32, lower_with_declarations};
    use proc_macro2::TokenStream;
    use quote::quote;

    use crate::experimental::{expander, expansion::Expansion};

    #[test]
    fn expands_all_declaration_families_in_contract_order() {
        let source = source_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("contract lowers");
        let expansion = Expansion::new(&lowered);

        let tokens = expander::Expander::new(&source)
            .native(&expansion)
            .expect("contract expands");
        let rendered = tokens.to_string();

        assert_in_order(
            &rendered,
            &[
                "boltffi_register_callback_demo_listener",
                "unsafe impl :: boltffi :: __private :: Passable for Point",
                "unsafe impl :: boltffi :: __private :: Passable for Status",
                "fn boltffi_release_class_demo_engine",
                "fn boltffi_stream_demo_engine_values_subscribe",
                "fn boltffi_const_demo_magic",
                "fn boltffi_function_demo_answer",
            ],
        );
        assert_generated_crate_checks("expander_all_declarations", full_contract_crate(tokens));
    }

    #[test]
    fn renders_ownerless_stream_from_free_function() {
        let source = ownerless_stream_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("contract lowers");
        let expansion = Expansion::new(&lowered);

        let tokens = expander::Expander::new(&source)
            .native(&expansion)
            .expect("contract expands");
        let rendered = tokens.to_string();

        assert!(rendered.contains("fn boltffi_stream_demo_events_subscribe () -> u64"));
        assert!(rendered.contains("let subscription = events ()"));
        assert_generated_crate_checks("expander_ownerless_stream", ownerless_stream_crate(tokens));
    }

    #[test]
    fn expands_native_and_wasm_surfaces_together() {
        let source = ownerless_stream_contract();
        let native_lowered = lower_with_declarations::<Native>(&source).expect("native lowers");
        let wasm32_lowered = lower_with_declarations::<Wasm32>(&source).expect("wasm lowers");
        let native = Expansion::new(&native_lowered);
        let wasm32 = Expansion::new(&wasm32_lowered);

        let tokens = expander::Expander::new(&source)
            .all(&native, &wasm32)
            .expect("all surfaces expand");
        let rendered = tokens.to_string();

        assert!(rendered.contains("# [cfg (not (target_arch = \"wasm32\"))]"));
        assert!(rendered.contains("# [cfg (target_arch = \"wasm32\")]"));
        assert_generated_crate_checks("expander_all_surfaces", ownerless_stream_crate(tokens));
    }

    fn source_contract() -> SourceContract {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener_trait());
        source.records.push(point_record());
        source.enums.push(status_enum());
        source.classes.push(engine_class());
        source.streams.push(stream());
        source.constants.push(bytes_constant());
        source.functions.push(answer_function());
        source
    }

    fn ownerless_stream_contract() -> SourceContract {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.streams.push(StreamDef::new(
            StreamId::new("demo::events"),
            CanonicalName::single("events"),
            TypeExpr::Primitive(Primitive::U32),
        ));
        source
    }

    fn listener_trait() -> TraitDef {
        let mut listener = TraitDef::new(
            TraitId::new("demo::Listener"),
            CanonicalName::single("Listener"),
        );
        let mut method = MethodDef::new(
            MethodId::new("on_value"),
            CanonicalName::single("on_value"),
            Receiver::Shared,
        );
        method.parameters = vec![ParameterDef::value(
            CanonicalName::single("value"),
            TypeExpr::Primitive(Primitive::U32),
        )];
        method.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));
        listener.methods.push(method);
        listener
    }

    fn point_record() -> RecordDef {
        let mut record = RecordDef::new("demo::Point".into(), CanonicalName::single("Point"));
        record.repr = ReprAttr::new(vec![ReprItem::C]);
        record.fields = vec![FieldDef::new(
            CanonicalName::single("x"),
            TypeExpr::Primitive(Primitive::F64),
        )];
        record
    }

    fn status_enum() -> EnumDef {
        let mut enumeration =
            EnumDef::new(EnumId::new("demo::Status"), CanonicalName::single("Status"));
        enumeration.variants = vec![
            VariantDef::unit(SourceName::new("Ready", CanonicalName::single("Ready"))),
            VariantDef::unit(SourceName::new("Done", CanonicalName::single("Done"))),
        ];
        enumeration
    }

    fn engine_class() -> ClassDef {
        ClassDef::new("demo::Engine".into(), CanonicalName::single("Engine"))
    }

    fn stream() -> StreamDef {
        let mut stream = StreamDef::new(
            StreamId::new("demo::Engine::values"),
            CanonicalName::single("values"),
            TypeExpr::Primitive(Primitive::U32),
        );
        stream.owner = Some("demo::Engine".into());
        stream
    }

    fn bytes_constant() -> ConstantDef {
        ConstantDef::new(
            ConstantId::new("demo::MAGIC"),
            SourceName::new("MAGIC", CanonicalName::single("MAGIC")),
            TypeExpr::slice(TypeExpr::Primitive(Primitive::U8)),
            ConstExpr::Literal(Literal::Bytes(b"ffi".to_vec())),
        )
    }

    fn answer_function() -> FunctionDef {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::answer"),
            CanonicalName::single("answer"),
        );
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));
        function
    }

    fn assert_in_order(source: &str, needles: &[&str]) {
        needles
            .iter()
            .try_fold(0usize, |offset, needle| {
                source[offset..]
                    .find(needle)
                    .map(|position| offset + position + needle.len())
                    .ok_or(needle)
            })
            .expect("rendered contract preserves declaration order");
    }

    fn full_contract_crate(tokens: TokenStream) -> TokenStream {
        quote! {
            #![allow(dead_code)]

            use std::sync::Arc;
            use boltffi::{EventSubscription, StreamProducer};

            pub trait Listener {
                fn on_value(&self, value: u32) -> u32;
            }

            #[derive(Clone, Copy)]
            #[repr(C)]
            pub struct Point {
                pub x: f64,
            }

            pub enum Status {
                Ready,
                Done,
            }

            pub struct Engine {
                producer: StreamProducer<u32>,
            }

            impl Engine {
                pub fn values(&self) -> Arc<EventSubscription<u32>> {
                    self.producer.subscribe()
                }
            }

            pub const MAGIC: &[u8] = b"ffi";

            pub fn answer() -> u32 {
                42
            }

            #tokens
        }
    }

    fn ownerless_stream_crate(tokens: TokenStream) -> TokenStream {
        quote! {
            #![allow(dead_code)]

            use std::sync::Arc;
            use boltffi::{EventSubscription, StreamProducer};

            pub fn events() -> Arc<EventSubscription<u32>> {
                StreamProducer::<u32>::new(16).subscribe()
            }

            #tokens
        }
    }

    fn assert_generated_crate_checks(name: &str, code: TokenStream) {
        let generated_crate = GeneratedCrate::create(name);
        generated_crate.write(code);
        generated_crate.check();
    }

    struct GeneratedCrate {
        root: PathBuf,
    }

    impl GeneratedCrate {
        fn create(name: &str) -> Self {
            if cfg!(miri) {
                return Self {
                    root: PathBuf::new(),
                };
            }
            let root = workspace_root()
                .join("target")
                .join("experimental-expander-checks")
                .join(format!("{}-{}", name, std::process::id()));
            if root.exists() {
                fs::remove_dir_all(&root).expect("remove old generated crate");
            }
            fs::create_dir_all(root.join("src")).expect("create generated crate");
            Self { root }
        }

        fn write(&self, code: TokenStream) {
            if cfg!(miri) {
                return;
            }
            fs::write(self.root.join("Cargo.toml"), self.manifest()).expect("write Cargo.toml");
            fs::write(self.root.join("src/lib.rs"), code.to_string()).expect("write lib.rs");
        }

        fn check(&self) {
            if cfg!(miri) {
                return;
            }
            let output = Command::new(cargo())
                .arg("check")
                .arg("--quiet")
                .arg("--manifest-path")
                .arg(self.root.join("Cargo.toml"))
                .env(
                    "CARGO_TARGET_DIR",
                    workspace_root()
                        .join("target")
                        .join("experimental-expander-checks-target"),
                )
                .output()
                .expect("run cargo check for generated crate");
            assert!(
                output.status.success(),
                "generated crate failed to check\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        fn manifest(&self) -> String {
            format!(
                "[package]\nname = \"generated_expander_check\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[workspace]\n\n[dependencies]\nboltffi = {{ path = \"{}\" }}\n",
                workspace_root().join("boltffi").display()
            )
        }
    }

    fn workspace_root() -> PathBuf {
        FsPath::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .to_path_buf()
    }

    fn cargo() -> OsString {
        std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
    }
}

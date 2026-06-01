use boltffi_ast::{FunctionDef, SourceContract};
use boltffi_binding::{FunctionDecl, LoweredBindings, Surface};
use proc_macro2::TokenStream;
use syn::ItemFn;

use super::decl::DeclarationPair;
use super::error::Error;
use super::index::ExpansionIndex;
use super::syntax::{self, Expand};
use super::target::Target;

pub struct Expansion<'a, S: Surface> {
    source: &'a SourceContract,
    lowered: &'a LoweredBindings<S>,
    index: ExpansionIndex,
}

impl<'a, S: Surface> Expansion<'a, S> {
    pub fn new(source: &'a SourceContract, lowered: &'a LoweredBindings<S>) -> Self {
        Self {
            source,
            lowered,
            index: ExpansionIndex::new(lowered.bindings()),
        }
    }

    pub fn source(&self) -> &'a SourceContract {
        self.source
    }

    pub fn bindings(&self) -> &'a boltffi_binding::Bindings<S> {
        self.lowered.bindings()
    }
}

impl<'a, S: Target> Expansion<'a, S> {
    pub fn pair<I>(
        &self,
        source: &'a I::Source,
    ) -> Result<DeclarationPair<'a, I::Source, I::Binding>, Error>
    where
        I: Expand<'a, S>,
    {
        I::pair(self.index.paired(self.lowered, I::source(source))?)
    }

    pub fn expand<I>(&self, source: &'a I::Source, syntax: I) -> Result<TokenStream, Error>
    where
        I: Expand<'a, S> + 'a,
    {
        let pair = self.pair::<I>(source)?;
        syntax.render(pair)
    }

    pub fn function(&self, source: &'a FunctionDef, syntax: ItemFn) -> Result<TokenStream, Error>
    where
        syntax::function::ExpandableFunction:
            Expand<'a, S, Source = FunctionDef, Binding = FunctionDecl<S>>,
    {
        self.expand(source, syntax::function::ExpandableFunction::new(syntax))
    }
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        CanonicalName, FieldDef, FunctionDef, FunctionId, PackageInfo, ParameterDef,
        ParameterPassing, Primitive, RecordDef, ReturnDef, SourceContract, TypeExpr,
    };
    use boltffi_binding::{Native, Wasm32, lower_with_declarations};
    use quote::quote;

    use super::Expansion;

    fn source_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::answer"),
            CanonicalName::single("answer"),
        );
        function.returns = ReturnDef::Value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn void_source_contract() -> SourceContract {
        let function =
            FunctionDef::new(FunctionId::new("demo::ping"), CanonicalName::single("ping"));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn point_record() -> RecordDef {
        let mut record = RecordDef::new("demo::Point".into(), CanonicalName::single("Point"));
        record.fields = vec![FieldDef::new(
            CanonicalName::single("x"),
            TypeExpr::Primitive(Primitive::F64),
        )];
        record
    }

    fn direct_record_param_contract() -> SourceContract {
        let mut function =
            FunctionDef::new(FunctionId::new("demo::norm"), CanonicalName::single("norm"));
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("point"),
            TypeExpr::Record("demo::Point".into()),
        )];
        function.returns = ReturnDef::Value(TypeExpr::Primitive(Primitive::F64));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(point_record());
        source.functions.push(function);
        source
    }

    fn direct_record_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::origin"),
            CanonicalName::single("origin"),
        );
        function.returns = ReturnDef::Value(TypeExpr::Record("demo::Point".into()));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(point_record());
        source.functions.push(function);
        source
    }

    fn string_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::greet"),
            CanonicalName::single("greet"),
        );
        function.returns = ReturnDef::Value(TypeExpr::String);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn string_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::name_len"),
            CanonicalName::single("name_len"),
        );
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("name"),
            TypeExpr::String,
        )];
        function.returns = ReturnDef::Value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn borrowed_string_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::name_len"),
            CanonicalName::single("name_len"),
        );
        let mut parameter = ParameterDef::value(CanonicalName::single("name"), TypeExpr::String);
        parameter.passing = ParameterPassing::Ref;
        function.parameters = vec![parameter];
        function.returns = ReturnDef::Value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn bytes_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::bytes_len"),
            CanonicalName::single("bytes_len"),
        );
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("bytes"),
            TypeExpr::Bytes,
        )];
        function.returns = ReturnDef::Value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    #[test]
    fn function_expansion_uses_exact_source_declaration() {
        let source = source_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&source, &lowered);
        let syntax = syn::parse_quote! {
            pub fn answer() -> u32 {
                42
            }
        };

        let tokens = expansion
            .function(&source.functions[0], syntax)
            .expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn answer() -> u32 {
                    42
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_answer() -> u32 {
                    answer()
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_function_expansion_uses_wasm_cfg() {
        let source = source_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&source, &lowered);
        let syntax = syn::parse_quote! {
            pub fn answer() -> u32 {
                42
            }
        };

        let tokens = expansion
            .function(&source.functions[0], syntax)
            .expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn answer() -> u32 {
                    42
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_answer() -> u32 {
                    answer()
                }
            }
            .to_string()
        );
    }

    #[test]
    fn void_function_expansion_returns_status() {
        let source = void_source_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&source, &lowered);
        let syntax = syn::parse_quote! {
            pub fn ping() {}
        };

        let tokens = expansion
            .function(&source.functions[0], syntax)
            .expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn ping() {}
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_ping() -> ::boltffi::__private::FfiStatus {
                    ping();
                    ::boltffi::__private::FfiStatus::OK
                }
            }
            .to_string()
        );
    }

    #[test]
    fn direct_record_param_expansion_unpacks_passable_input() {
        let source = direct_record_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&source, &lowered);
        let syntax = syn::parse_quote! {
            pub fn norm(point: Point) -> f64 {
                point.x
            }
        };

        let tokens = expansion
            .function(&source.functions[0], syntax)
            .expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn norm(point: Point) -> f64 {
                    point.x
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_norm(
                    point: <Point as ::boltffi::__private::Passable>::In
                ) -> f64 {
                    let point: Point = unsafe {
                        <Point as ::boltffi::__private::Passable>::unpack(point)
                    };
                    norm(point)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_string_param_expansion_decodes_owned_string() {
        let source = string_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&source, &lowered);
        let syntax = syn::parse_quote! {
            pub fn name_len(name: String) -> u32 {
                name.len() as u32
            }
        };

        let tokens = expansion
            .function(&source.functions[0], syntax)
            .expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn name_len(name: String) -> u32 {
                    name.len() as u32
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_name_len(
                    __boltffi_name_ptr: *const u8,
                    __boltffi_name_len: usize
                ) -> u32 {
                    let name: String = if __boltffi_name_ptr.is_null() {
                        String::new()
                    } else {
                        match ::core::str::from_utf8(unsafe {
                            ::core::slice::from_raw_parts(__boltffi_name_ptr, __boltffi_name_len)
                        }) {
                            Ok(value) => value.to_string(),
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: invalid UTF-8: {} (buf_len={})",
                                    stringify!(name),
                                    error,
                                    __boltffi_name_len
                                ));
                                String::new()
                            }
                        }
                    };
                    name_len(name)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_borrowed_string_param_expansion_decodes_str_ref() {
        let source = borrowed_string_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&source, &lowered);
        let syntax = syn::parse_quote! {
            pub fn name_len(name: &str) -> u32 {
                name.len() as u32
            }
        };

        let tokens = expansion
            .function(&source.functions[0], syntax)
            .expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn name_len(name: &str) -> u32 {
                    name.len() as u32
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_name_len(
                    __boltffi_name_ptr: *const u8,
                    __boltffi_name_len: usize
                ) -> u32 {
                    let name: &str = if __boltffi_name_ptr.is_null() {
                        ""
                    } else {
                        match ::core::str::from_utf8(unsafe {
                            ::core::slice::from_raw_parts(__boltffi_name_ptr, __boltffi_name_len)
                        }) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: invalid UTF-8: {} (buf_len={})",
                                    stringify!(name),
                                    error,
                                    __boltffi_name_len
                                ));
                                ""
                            }
                        }
                    };
                    name_len(name)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_bytes_param_expansion_decodes_owned_bytes() {
        let source = bytes_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&source, &lowered);
        let syntax = syn::parse_quote! {
            pub fn bytes_len(bytes: Vec<u8>) -> u32 {
                bytes.len() as u32
            }
        };

        let tokens = expansion
            .function(&source.functions[0], syntax)
            .expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn bytes_len(bytes: Vec<u8>) -> u32 {
                    bytes.len() as u32
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_bytes_len(
                    __boltffi_bytes_ptr: *const u8,
                    __boltffi_bytes_len: usize
                ) -> u32 {
                    let bytes: Vec<u8> = if __boltffi_bytes_ptr.is_null() {
                        Vec::new()
                    } else {
                        unsafe {
                            ::core::slice::from_raw_parts(
                                __boltffi_bytes_ptr,
                                __boltffi_bytes_len
                            )
                        }.to_vec()
                    };
                    bytes_len(bytes)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn direct_record_return_expansion_packs_passable_output() {
        let source = direct_record_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&source, &lowered);
        let syntax = syn::parse_quote! {
            pub fn origin() -> Point {
                Point { x: 0.0 }
            }
        };

        let tokens = expansion
            .function(&source.functions[0], syntax)
            .expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn origin() -> Point {
                    Point { x: 0.0 }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_origin() -> <Point as ::boltffi::__private::Passable>::Out {
                    ::boltffi::__private::Passable::pack(origin())
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_string_return_expansion_returns_buffer() {
        let source = string_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&source, &lowered);
        let syntax = syn::parse_quote! {
            pub fn greet() -> String {
                String::from("hello")
            }
        };

        let tokens = expansion
            .function(&source.functions[0], syntax)
            .expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn greet() -> String {
                    String::from("hello")
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_greet() -> ::boltffi::__private::FfiBuf {
                    let __boltffi_result: String = greet();
                    ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_result)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_string_return_expansion_returns_packed_buffer() {
        let source = string_return_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&source, &lowered);
        let syntax = syn::parse_quote! {
            pub fn greet() -> String {
                String::from("hello")
            }
        };

        let tokens = expansion
            .function(&source.functions[0], syntax)
            .expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn greet() -> String {
                    String::from("hello")
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_greet() -> u64 {
                    let __boltffi_result: String = greet();
                    ::boltffi::__private::FfiBuf::from_vec(__boltffi_result.into_bytes()).into_packed()
                }
            }
            .to_string()
        );
    }
}

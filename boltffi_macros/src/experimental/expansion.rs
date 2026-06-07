use boltffi_binding::{LoweredBindings, Surface};

use super::error::Error;
use super::index::ExpansionIndex;
use super::syntax::ExpandableDeclaration;

/// An indexed lowered crate for one target surface.
///
/// The value pairs scanned source declarations with their lowered binding declarations.
/// It does not render Rust syntax, choose target sets, scan source, or run lowering.
pub struct Expansion<'a, S: Surface> {
    lowered: &'a LoweredBindings<S>,
    index: ExpansionIndex,
}

impl<'a, S: Surface> Expansion<'a, S> {
    /// Creates an indexed view over lowered bindings for one target surface.
    pub fn new(lowered: &'a LoweredBindings<S>) -> Self {
        Self {
            lowered,
            index: ExpansionIndex::new(lowered.bindings()),
        }
    }

    /// Returns the lowered binding declarations.
    pub fn bindings(&self) -> &'a boltffi_binding::Bindings<S> {
        self.lowered.bindings()
    }

    /// Returns the lowered declaration paired with the given source declaration.
    pub fn declaration<I>(&self, source: &'a I::Source) -> Result<I::Binding<'a, S>, Error>
    where
        I: ExpandableDeclaration,
    {
        I::binding(self.index.paired(self.lowered, I::source(source))?)
    }
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        CanonicalName, ClassDef, ClassId, ExecutionKind, FieldDef, FnSig, FnTrait, FnTraitKind,
        FunctionDef, FunctionId, PackageInfo, ParameterDef, ParameterPassing, Path, Primitive,
        RecordDef, RecordId, ReturnDef, Source, SourceContract, SourceName, TraitDef, TraitId,
        TypeExpr, Visibility,
    };
    use boltffi_binding::{Native, Wasm32, lower_with_declarations};
    use proc_macro2::TokenStream;
    use quote::quote;
    use syn::ItemFn;

    use super::Expansion;
    use crate::experimental::error::Error;
    use crate::experimental::render;
    use crate::experimental::syntax::{ItemRenderer, RenderableItem, function::ExpandableFunction};
    use crate::experimental::target::Target;

    fn expand_function<'a, S>(
        expansion: &Expansion<'a, S>,
        source: &'a FunctionDef,
        syntax: ItemFn,
    ) -> Result<TokenStream, Error>
    where
        S: Target,
        render::function::Rule<'a, S>: ItemRenderer<'a, S, ExpandableFunction>,
    {
        let wrapper = ExpandableFunction::render(expansion, source, &syntax)?;

        Ok(quote! {
            #syntax
            #wrapper
        })
    }

    fn source_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::answer"),
            CanonicalName::single("answer"),
        );
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn source_name_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::http_request"),
            SourceName::new("HTTPRequest", CanonicalName::single("http_request")),
        );
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn source_parameter_name_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::syntax_payload"),
            CanonicalName::single("syntax_payload"),
        );
        function.parameters = vec![ParameterDef::value(
            SourceName::new("HTTPCode", CanonicalName::single("http_code")),
            TypeExpr::Primitive(Primitive::U32),
        )];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn source_visibility_contract(visibility: Visibility) -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::answer"),
            CanonicalName::single("answer"),
        );
        function.source = Source::new(visibility, None);
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn path(name: &str) -> Path {
        Path::single(name)
    }

    fn record(name: &str) -> TypeExpr {
        TypeExpr::record(RecordId::new(format!("demo::{name}")), path(name))
    }

    fn class(name: &str) -> TypeExpr {
        TypeExpr::class(ClassId::new(format!("demo::{name}")), path(name))
    }

    fn byte_slice() -> TypeExpr {
        TypeExpr::slice(TypeExpr::Primitive(Primitive::U8))
    }

    fn byte_vec() -> TypeExpr {
        TypeExpr::vec(TypeExpr::Primitive(Primitive::U8))
    }

    fn parameter(name: &str, expr: TypeExpr) -> ParameterDef {
        ParameterDef::value(CanonicalName::single(name), expr)
    }

    fn vector_parameter(name: &str, element: TypeExpr) -> ParameterDef {
        parameter(name, TypeExpr::vec(element))
    }

    fn result_return(ok: TypeExpr, error: TypeExpr) -> ReturnDef {
        ReturnDef::value(TypeExpr::result(ok, error))
    }

    fn fn_signature(parameters: Vec<TypeExpr>, returns: ReturnDef) -> FnSig {
        FnSig::new(parameters, returns)
    }

    fn fn_trait(parameters: Vec<TypeExpr>, returns: ReturnDef) -> FnTrait {
        FnTrait::new(FnTraitKind::Fn, fn_signature(parameters, returns))
    }

    fn impl_closure(parameters: Vec<TypeExpr>, returns: ReturnDef) -> TypeExpr {
        TypeExpr::impl_fn(fn_trait(parameters, returns))
    }

    fn boxed_closure(parameters: Vec<TypeExpr>, returns: ReturnDef) -> TypeExpr {
        TypeExpr::boxed(TypeExpr::dyn_fn(fn_trait(parameters, returns)))
    }

    fn function_pointer(parameters: Vec<TypeExpr>, returns: ReturnDef) -> TypeExpr {
        TypeExpr::fn_ptr(fn_signature(parameters, returns))
    }

    fn boxed_listener() -> TypeExpr {
        TypeExpr::boxed(TypeExpr::dyn_trait(
            TraitId::new("demo::Listener"),
            path("Listener"),
        ))
    }

    fn nullable_arc_listener() -> TypeExpr {
        TypeExpr::option(TypeExpr::arc(TypeExpr::dyn_trait(
            TraitId::new("demo::Listener"),
            path("Listener"),
        )))
    }

    fn void_source_contract() -> SourceContract {
        let function =
            FunctionDef::new(FunctionId::new("demo::ping"), CanonicalName::single("ping"));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn async_answer_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::answer"),
            CanonicalName::single("answer"),
        );
        function.execution = ExecutionKind::Async;
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn async_ping_contract() -> SourceContract {
        let mut function =
            FunctionDef::new(FunctionId::new("demo::ping"), CanonicalName::single("ping"));
        function.execution = ExecutionKind::Async;

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn async_greet_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::greet"),
            CanonicalName::single("greet"),
        );
        function.execution = ExecutionKind::Async;
        function.returns = ReturnDef::value(TypeExpr::String);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn async_string_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::name_len"),
            CanonicalName::single("name_len"),
        );
        function.execution = ExecutionKind::Async;
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("name"),
            TypeExpr::String,
        )];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn async_borrowed_string_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::name_len"),
            CanonicalName::single("name_len"),
        );
        function.execution = ExecutionKind::Async;
        let mut parameter = ParameterDef::value(CanonicalName::single("name"), TypeExpr::String);
        parameter.passing = ParameterPassing::Ref;
        function.parameters = vec![parameter];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn async_result_i32_string_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::try_count"),
            CanonicalName::single("try_count"),
        );
        function.execution = ExecutionKind::Async;
        function.returns = result_return(TypeExpr::Primitive(Primitive::I32), TypeExpr::String);

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

    fn profile_record() -> RecordDef {
        let mut record = RecordDef::new("demo::Profile".into(), CanonicalName::single("Profile"));
        record.fields = vec![FieldDef::new(
            CanonicalName::single("name"),
            TypeExpr::String,
        )];
        record
    }

    fn engine_class() -> ClassDef {
        ClassDef::new("demo::Engine".into(), CanonicalName::single("Engine"))
    }

    fn listener_trait() -> TraitDef {
        TraitDef::new(
            TraitId::new("demo::Listener"),
            CanonicalName::single("Listener"),
        )
    }

    fn closure_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::apply"),
            CanonicalName::single("apply"),
        );
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("callback"),
            impl_closure(
                vec![TypeExpr::Primitive(Primitive::U32)],
                ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
            ),
        )];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn string_closure_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::apply"),
            CanonicalName::single("apply"),
        );
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("callback"),
            impl_closure(vec![TypeExpr::String], ReturnDef::value(TypeExpr::String)),
        )];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn fallible_closure_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::apply"),
            CanonicalName::single("apply"),
        );
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("callback"),
            impl_closure(
                Vec::<TypeExpr>::new(),
                result_return(TypeExpr::Unit, TypeExpr::String),
            ),
        )];

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn fallible_i32_closure_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::apply"),
            CanonicalName::single("apply"),
        );
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("callback"),
            impl_closure(
                Vec::<TypeExpr>::new(),
                result_return(TypeExpr::Primitive(Primitive::I32), TypeExpr::String),
            ),
        )];

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn fallible_string_closure_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::apply"),
            CanonicalName::single("apply"),
        );
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("callback"),
            impl_closure(
                Vec::<TypeExpr>::new(),
                result_return(TypeExpr::String, TypeExpr::String),
            ),
        )];

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn closure_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::make_callback"),
            CanonicalName::single("make_callback"),
        );
        function.returns = ReturnDef::value(impl_closure(
            vec![TypeExpr::Primitive(Primitive::U32)],
            ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
        ));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn closure_return_with_closure_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::make_runner"),
            CanonicalName::single("make_runner"),
        );
        let callback = boxed_closure(
            vec![TypeExpr::Primitive(Primitive::U32)],
            ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
        );
        function.returns = ReturnDef::value(impl_closure(
            vec![callback],
            ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
        ));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn function_pointer_closure_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::make_callback"),
            CanonicalName::single("make_callback"),
        );
        function.returns = ReturnDef::value(function_pointer(
            vec![TypeExpr::Primitive(Primitive::U32)],
            ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
        ));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn string_closure_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::make_mapper"),
            CanonicalName::single("make_mapper"),
        );
        function.returns = ReturnDef::value(impl_closure(
            vec![TypeExpr::String],
            ReturnDef::value(TypeExpr::String),
        ));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn fallible_i32_closure_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::make_callback"),
            CanonicalName::single("make_callback"),
        );
        function.returns = ReturnDef::value(impl_closure(
            Vec::<TypeExpr>::new(),
            result_return(TypeExpr::Primitive(Primitive::I32), TypeExpr::String),
        ));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn fallible_string_closure_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::make_mapper"),
            CanonicalName::single("make_mapper"),
        );
        function.returns = ReturnDef::value(impl_closure(
            Vec::<TypeExpr>::new(),
            result_return(TypeExpr::String, TypeExpr::String),
        ));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn result_closure_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::try_make_callback"),
            CanonicalName::single("try_make_callback"),
        );
        let closure = impl_closure(
            vec![TypeExpr::Primitive(Primitive::U32)],
            ReturnDef::value(TypeExpr::Primitive(Primitive::U32)),
        );
        function.returns = result_return(closure, TypeExpr::String);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn direct_record_param_contract() -> SourceContract {
        let mut function =
            FunctionDef::new(FunctionId::new("demo::norm"), CanonicalName::single("norm"));
        function.parameters = vec![parameter("point", record("Point"))];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::F64));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(point_record());
        source.functions.push(function);
        source
    }

    fn mutable_direct_record_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::shift"),
            CanonicalName::single("shift"),
        );
        let mut parameter = parameter("point", record("Point"));
        parameter.passing = ParameterPassing::RefMut;
        function.parameters = vec![parameter];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::F64));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(point_record());
        source.functions.push(function);
        source
    }

    fn mutable_direct_primitive_param_contract() -> SourceContract {
        let mut function =
            FunctionDef::new(FunctionId::new("demo::bump"), CanonicalName::single("bump"));
        let mut parameter = parameter("count", TypeExpr::Primitive(Primitive::I32));
        parameter.passing = ParameterPassing::RefMut;
        function.parameters = vec![parameter];

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn direct_record_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::origin"),
            CanonicalName::single("origin"),
        );
        function.returns = ReturnDef::value(record("Point"));

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
        function.returns = ReturnDef::value(TypeExpr::String);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn bytes_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::payload"),
            CanonicalName::single("payload"),
        );
        function.returns = ReturnDef::value(byte_vec());

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
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn borrowed_string_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::name_len"),
            CanonicalName::single("name_len"),
        );
        let mut parameter = parameter("name", TypeExpr::Str);
        parameter.passing = ParameterPassing::Ref;
        function.parameters = vec![parameter];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn mutable_string_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::rewrite"),
            CanonicalName::single("rewrite"),
        );
        let mut parameter = parameter("name", TypeExpr::Str);
        parameter.passing = ParameterPassing::RefMut;
        function.parameters = vec![parameter];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

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
            byte_vec(),
        )];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn mutable_bytes_param_contract() -> SourceContract {
        let mut function =
            FunctionDef::new(FunctionId::new("demo::fill"), CanonicalName::single("fill"));
        let mut parameter = parameter("bytes", byte_slice());
        parameter.passing = ParameterPassing::RefMut;
        function.parameters = vec![parameter];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn encoded_record_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::name_score"),
            CanonicalName::single("name_score"),
        );
        function.parameters = vec![parameter("profile", record("Profile"))];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(profile_record());
        source.functions.push(function);
        source
    }

    fn mutable_encoded_record_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::rename"),
            CanonicalName::single("rename"),
        );
        let mut parameter = parameter("profile", record("Profile"));
        parameter.passing = ParameterPassing::RefMut;
        function.parameters = vec![parameter];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(profile_record());
        source.functions.push(function);
        source
    }

    fn option_i32_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::set_count"),
            CanonicalName::single("set_count"),
        );
        function.parameters = vec![ParameterDef::value(
            CanonicalName::single("count"),
            TypeExpr::option(TypeExpr::Primitive(Primitive::I32)),
        )];

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn vec_u32_param_contract() -> SourceContract {
        let mut function =
            FunctionDef::new(FunctionId::new("demo::sum"), CanonicalName::single("sum"));
        function.parameters = vec![vector_parameter(
            "values",
            TypeExpr::Primitive(Primitive::U32),
        )];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn vec_point_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::count_points"),
            CanonicalName::single("count_points"),
        );
        function.parameters = vec![vector_parameter("points", record("Point"))];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(point_record());
        source.functions.push(function);
        source
    }

    fn result_i32_string_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::try_count"),
            CanonicalName::single("try_count"),
        );
        function.returns = result_return(TypeExpr::Primitive(Primitive::I32), TypeExpr::String);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn result_unit_string_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::try_ping"),
            CanonicalName::single("try_ping"),
        );
        function.returns = result_return(TypeExpr::Unit, TypeExpr::String);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn result_string_string_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::try_greet"),
            CanonicalName::single("try_greet"),
        );
        function.returns = result_return(TypeExpr::String, TypeExpr::String);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn class_param_nullable_return_contract() -> SourceContract {
        let mut function =
            FunctionDef::new(FunctionId::new("demo::open"), CanonicalName::single("open"));
        function.parameters = vec![parameter("engine", class("Engine"))];
        function.returns = ReturnDef::value(TypeExpr::option(class("Engine")));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.classes.push(engine_class());
        source.functions.push(function);
        source
    }

    fn boxed_callback_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::listen"),
            CanonicalName::single("listen"),
        );
        function.parameters = vec![parameter("listener", boxed_listener())];

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener_trait());
        source.functions.push(function);
        source
    }

    fn nullable_arc_callback_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::maybe"),
            CanonicalName::single("maybe"),
        );
        function.parameters = vec![parameter("listener", nullable_arc_listener())];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.traits.push(listener_trait());
        source.functions.push(function);
        source
    }

    fn borrowed_class_param_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::engine_id"),
            CanonicalName::single("engine_id"),
        );
        let mut parameter = parameter("engine", class("Engine"));
        parameter.passing = ParameterPassing::Ref;
        function.parameters = vec![parameter];
        function.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.classes.push(engine_class());
        source.functions.push(function);
        source
    }

    fn result_class_string_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::try_open"),
            CanonicalName::single("try_open"),
        );
        function.returns = result_return(class("Engine"), TypeExpr::String);

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.classes.push(engine_class());
        source.functions.push(function);
        source
    }

    fn option_i32_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::maybe_count"),
            CanonicalName::single("maybe_count"),
        );
        function.returns = ReturnDef::value(TypeExpr::option(TypeExpr::Primitive(Primitive::I32)));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    fn vec_i32_return_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::numbers"),
            CanonicalName::single("numbers"),
        );
        function.returns = ReturnDef::value(TypeExpr::vec(TypeExpr::Primitive(Primitive::I32)));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    #[test]
    fn function_expansion_uses_exact_source_declaration() {
        let source = source_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn answer() -> u32 {
                42
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

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
    fn function_expansion_invokes_source_name_spelling() {
        let source = source_name_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn syntax_payload() -> u32 {
                42
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn syntax_payload() -> u32 {
                    42
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_http_request() -> u32 {
                    HTTPRequest()
                }
            }
            .to_string()
        );
    }

    #[test]
    fn function_expansion_uses_source_parameter_name_spelling() {
        let source = source_parameter_name_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn syntax_payload(value: u32) -> u32 {
                value
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn syntax_payload(value: u32) -> u32 {
                    value
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_syntax_payload(
                    HTTPCode: u32
                ) -> u32 {
                    syntax_payload(HTTPCode)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn function_expansion_uses_private_source_visibility() {
        let source = source_visibility_contract(Visibility::Private);
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn answer() -> u32 {
                42
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn answer() -> u32 {
                    42
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                extern "C" fn boltffi_function_demo_answer() -> u32 {
                    answer()
                }
            }
            .to_string()
        );
    }

    #[test]
    fn function_expansion_uses_restricted_source_visibility() {
        let source = source_visibility_contract(Visibility::Restricted("crate".to_owned()));
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn answer() -> u32 {
                42
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn answer() -> u32 {
                    42
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub(in crate) extern "C" fn boltffi_function_demo_answer() -> u32 {
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
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn answer() -> u32 {
                42
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

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
    fn function_wrappers_compose_native_and_wasm_without_reemitting_source_item() {
        let source = source_contract();
        let native_lowered =
            lower_with_declarations::<Native>(&source).expect("native lowered bindings");
        let wasm_lowered =
            lower_with_declarations::<Wasm32>(&source).expect("wasm lowered bindings");
        let native_expansion = Expansion::new(&native_lowered);
        let wasm_expansion = Expansion::new(&wasm_lowered);
        let syntax = syn::parse_quote! {
            pub fn answer() -> u32 {
                42
            }
        };

        let native_wrapper =
            ExpandableFunction::render(&native_expansion, &source.functions[0], &syntax)
                .expect("native wrapper");
        let wasm_wrapper =
            ExpandableFunction::render(&wasm_expansion, &source.functions[0], &syntax)
                .expect("wasm wrapper");

        let tokens = quote! {
            #syntax
            #native_wrapper
            #wasm_wrapper
        };

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
    fn native_async_function_expansion_exports_poll_handle_lifecycle() {
        let source = async_answer_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub async fn answer() -> u32 {
                42
            }
        };

        let tokens = expand_function(&expansion, &source.functions[0], syntax)
            .expect("expanded async function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub async fn answer() -> u32 {
                    42
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_answer() -> ::boltffi::__private::RustFutureHandle {
                    ::boltffi::__private::rustfuture::rust_future_new(async move {
                        answer().await
                    })
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_answer_poll(
                    handle: ::boltffi::__private::RustFutureHandle,
                    callback_data: u64,
                    callback: ::boltffi::__private::RustFutureContinuationCallback,
                ) {
                    ::boltffi::__private::rustfuture::rust_future_poll::<u32>(
                        handle,
                        callback,
                        callback_data
                    )
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_answer_complete(
                    handle: ::boltffi::__private::RustFutureHandle,
                    out_status: *mut ::boltffi::__private::FfiStatus,
                ) -> u32 {
                    match ::boltffi::__private::rustfuture::rust_future_complete::<u32>(handle) {
                        Ok(result) => {
                            if !out_status.is_null() {
                                *out_status = ::boltffi::__private::FfiStatus::OK;
                            }
                            result
                        }
                        Err(status) => {
                            if !out_status.is_null() {
                                *out_status = status;
                            }
                            Default::default()
                        }
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_answer_panic_message(
                    handle: ::boltffi::__private::RustFutureHandle,
                ) -> ::boltffi::__private::FfiBuf {
                    match ::boltffi::__private::rustfuture::rust_future_panic_message::<u32>(handle) {
                        Some(message) => ::boltffi::__private::FfiBuf::wire_encode(&message),
                        None => ::boltffi::__private::FfiBuf::empty(),
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_answer_cancel(
                    handle: ::boltffi::__private::RustFutureHandle
                ) {
                    ::boltffi::__private::rustfuture::rust_future_cancel::<u32>(handle)
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_answer_free(
                    handle: ::boltffi::__private::RustFutureHandle
                ) {
                    ::boltffi::__private::rustfuture::rust_future_free::<u32>(handle)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_async_encoded_return_expansion_completes_with_encoded_value() {
        let source = async_greet_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub async fn greet() -> String {
                String::from("hello")
            }
        };

        let tokens = expand_function(&expansion, &source.functions[0], syntax)
            .expect("expanded async function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub async fn greet() -> String {
                    String::from("hello")
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_greet() -> ::boltffi::__private::RustFutureHandle {
                    ::boltffi::__private::rustfuture::rust_future_new(async move {
                        greet().await
                    })
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_greet_poll(
                    handle: ::boltffi::__private::RustFutureHandle,
                    callback_data: u64,
                    callback: ::boltffi::__private::RustFutureContinuationCallback,
                ) {
                    ::boltffi::__private::rustfuture::rust_future_poll::<String>(
                        handle,
                        callback,
                        callback_data
                    )
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_greet_complete(
                    handle: ::boltffi::__private::RustFutureHandle,
                    out_status: *mut ::boltffi::__private::FfiStatus,
                ) -> ::boltffi::__private::FfiBuf {
                    match ::boltffi::__private::rustfuture::rust_future_complete::<String>(handle) {
                        Ok(__boltffi_result) => {
                            if !out_status.is_null() {
                                *out_status = ::boltffi::__private::FfiStatus::OK;
                            }
                            ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_result)
                        }
                        Err(status) => {
                            if !out_status.is_null() {
                                *out_status = status;
                            }
                            ::boltffi::__private::FfiBuf::default()
                        }
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_greet_panic_message(
                    handle: ::boltffi::__private::RustFutureHandle,
                ) -> ::boltffi::__private::FfiBuf {
                    match ::boltffi::__private::rustfuture::rust_future_panic_message::<String>(handle) {
                        Some(message) => ::boltffi::__private::FfiBuf::wire_encode(&message),
                        None => ::boltffi::__private::FfiBuf::empty(),
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_greet_cancel(
                    handle: ::boltffi::__private::RustFutureHandle
                ) {
                    ::boltffi::__private::rustfuture::rust_future_cancel::<String>(handle)
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_greet_free(
                    handle: ::boltffi::__private::RustFutureHandle
                ) {
                    ::boltffi::__private::rustfuture::rust_future_free::<String>(handle)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_async_result_expansion_writes_success_out_pointer_and_encoded_error() {
        let source = async_result_i32_string_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub async fn try_count() -> Result<i32, String> {
                Ok(7)
            }
        };

        let tokens = expand_function(&expansion, &source.functions[0], syntax)
            .expect("expanded async function");
        let rust_return_type: syn::Type = syn::parse_quote! { Result<i32, String> };

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub async fn try_count() -> Result<i32, String> {
                    Ok(7)
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_try_count() -> ::boltffi::__private::RustFutureHandle {
                    ::boltffi::__private::rustfuture::rust_future_new(async move {
                        try_count().await
                    })
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_try_count_poll(
                    handle: ::boltffi::__private::RustFutureHandle,
                    callback_data: u64,
                    callback: ::boltffi::__private::RustFutureContinuationCallback,
                ) {
                    ::boltffi::__private::rustfuture::rust_future_poll::<#rust_return_type>(
                        handle,
                        callback,
                        callback_data
                    )
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_try_count_complete(
                    handle: ::boltffi::__private::RustFutureHandle,
                    out_status: *mut ::boltffi::__private::FfiStatus,
                    __boltffi_return_out: *mut i32
                ) -> ::boltffi::__private::FfiBuf {
                    match ::boltffi::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle) {
                        Ok(Ok(__boltffi_success)) => {
                            if !out_status.is_null() {
                                *out_status = ::boltffi::__private::FfiStatus::OK;
                            }
                            if !__boltffi_return_out.is_null() {
                                unsafe {
                                    *__boltffi_return_out = __boltffi_success;
                                }
                            }
                            ::boltffi::__private::FfiBuf::default()
                        }
                        Ok(Err(__boltffi_error)) => {
                            if !out_status.is_null() {
                                *out_status = ::boltffi::__private::FfiStatus::OK;
                            }
                            ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_error)
                        }
                        Err(status) => {
                            if !out_status.is_null() {
                                *out_status = status;
                            }
                            ::boltffi::__private::FfiBuf::default()
                        }
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_try_count_panic_message(
                    handle: ::boltffi::__private::RustFutureHandle,
                ) -> ::boltffi::__private::FfiBuf {
                    match ::boltffi::__private::rustfuture::rust_future_panic_message::<#rust_return_type>(handle) {
                        Some(message) => ::boltffi::__private::FfiBuf::wire_encode(&message),
                        None => ::boltffi::__private::FfiBuf::empty(),
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_try_count_cancel(
                    handle: ::boltffi::__private::RustFutureHandle
                ) {
                    ::boltffi::__private::rustfuture::rust_future_cancel::<#rust_return_type>(handle)
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_try_count_free(
                    handle: ::boltffi::__private::RustFutureHandle
                ) {
                    ::boltffi::__private::rustfuture::rust_future_free::<#rust_return_type>(handle)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_async_void_function_expansion_exports_sync_poll_lifecycle() {
        let source = async_ping_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub async fn ping() {}
        };

        let tokens = expand_function(&expansion, &source.functions[0], syntax)
            .expect("expanded async function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub async fn ping() {}
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_ping() -> ::boltffi::__private::RustFutureHandle {
                    ::boltffi::__private::rustfuture::rust_future_new(async move {
                        ping().await
                    })
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_ping_poll_sync(
                    handle: ::boltffi::__private::RustFutureHandle,
                ) -> i32 {
                    ::boltffi::__private::rust_future_poll_sync::<()>(handle)
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_ping_complete(
                    handle: ::boltffi::__private::RustFutureHandle,
                    out_status: *mut ::boltffi::__private::FfiStatus,
                ) {
                    match ::boltffi::__private::rustfuture::rust_future_complete::<()>(handle) {
                        Ok(_) => {
                            if !out_status.is_null() {
                                *out_status = ::boltffi::__private::FfiStatus::OK;
                            }
                        }
                        Err(status) => {
                            if !out_status.is_null() {
                                *out_status = status;
                            }
                        }
                    }
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_ping_panic_message(
                    handle: ::boltffi::__private::RustFutureHandle,
                ) -> ::boltffi::__private::FfiBuf {
                    match ::boltffi::__private::rustfuture::rust_future_panic_message::<()>(handle) {
                        Some(message) => ::boltffi::__private::FfiBuf::wire_encode(&message),
                        None => ::boltffi::__private::FfiBuf::empty(),
                    }
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_ping_cancel(
                    handle: ::boltffi::__private::RustFutureHandle
                ) {
                    ::boltffi::__private::rustfuture::rust_future_cancel::<()>(handle)
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_ping_free(
                    handle: ::boltffi::__private::RustFutureHandle
                ) {
                    ::boltffi::__private::rustfuture::rust_future_free::<()>(handle)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_async_owned_string_param_expansion_decodes_before_spawning_future() {
        let source = async_string_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub async fn name_len(name: String) -> u32 {
                name.len() as u32
            }
        };

        let tokens = expand_function(&expansion, &source.functions[0], syntax)
            .expect("expanded async function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub async fn name_len(name: String) -> u32 {
                    name.len() as u32
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_name_len(
                    __boltffi_name_ptr: *const u8,
                    __boltffi_name_len: usize
                ) -> ::boltffi::__private::RustFutureHandle {
                    let name: String = {
                        if __boltffi_name_ptr.is_null() && __boltffi_name_len > 0 {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: null pointer with non-zero length (buf_len={})",
                                stringify!(name),
                                __boltffi_name_len
                            ));
                            return ::boltffi::__private::rustfuture::rust_future_invalid_arg::<u32>();
                        }
                        let __boltffi_bytes: &[u8] = if __boltffi_name_len == 0 {
                            &[]
                        } else {
                            unsafe {
                                ::core::slice::from_raw_parts(
                                    __boltffi_name_ptr,
                                    __boltffi_name_len
                                )
                            }
                        };
                        match ::boltffi::__private::wire::decode::<String>(__boltffi_bytes) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: wire decode failed: {} (buf_len={})",
                                    stringify!(name),
                                    error,
                                    __boltffi_name_len
                                ));
                                return ::boltffi::__private::rustfuture::rust_future_invalid_arg::<u32>();
                            }
                        }
                    };
                    ::boltffi::__private::rustfuture::rust_future_new(async move {
                        name_len(name).await
                    })
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_name_len_poll(
                    handle: ::boltffi::__private::RustFutureHandle,
                    callback_data: u64,
                    callback: ::boltffi::__private::RustFutureContinuationCallback,
                ) {
                    ::boltffi::__private::rustfuture::rust_future_poll::<u32>(
                        handle,
                        callback,
                        callback_data
                    )
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_name_len_complete(
                    handle: ::boltffi::__private::RustFutureHandle,
                    out_status: *mut ::boltffi::__private::FfiStatus,
                ) -> u32 {
                    match ::boltffi::__private::rustfuture::rust_future_complete::<u32>(handle) {
                        Ok(result) => {
                            if !out_status.is_null() {
                                *out_status = ::boltffi::__private::FfiStatus::OK;
                            }
                            result
                        }
                        Err(status) => {
                            if !out_status.is_null() {
                                *out_status = status;
                            }
                            Default::default()
                        }
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_name_len_panic_message(
                    handle: ::boltffi::__private::RustFutureHandle,
                ) -> ::boltffi::__private::FfiBuf {
                    match ::boltffi::__private::rustfuture::rust_future_panic_message::<u32>(handle) {
                        Some(message) => ::boltffi::__private::FfiBuf::wire_encode(&message),
                        None => ::boltffi::__private::FfiBuf::empty(),
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_name_len_cancel(
                    handle: ::boltffi::__private::RustFutureHandle
                ) {
                    ::boltffi::__private::rustfuture::rust_future_cancel::<u32>(handle)
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_async_function_demo_name_len_free(
                    handle: ::boltffi::__private::RustFutureHandle
                ) {
                    ::boltffi::__private::rustfuture::rust_future_free::<u32>(handle)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn async_borrowed_param_expansion_rejects_reference_capture() {
        let source = async_borrowed_string_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub async fn name_len(name: &str) -> u32 {
                name.len() as u32
            }
        };

        let error = expand_function(&expansion, &source.functions[0], syntax)
            .expect_err("async borrowed params must not capture FFI memory");

        assert_eq!(
            error.to_string(),
            "unsupported expansion: async reference parameter"
        );
    }

    #[test]
    fn void_function_expansion_returns_status() {
        let source = void_source_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn ping() {}
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

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
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn norm(point: Point) -> f64 {
                point.x
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

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
    fn wasm_direct_record_param_expansion_reads_writer_pointer() {
        let source = direct_record_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn norm(point: Point) -> f64 {
                point.x
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn norm(point: Point) -> f64 {
                    point.x
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_norm(
                    point: *const u8
                ) -> f64 {
                    if point.is_null() {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: null direct record pointer",
                            stringify!(point)
                        ));
                        return ::core::default::Default::default();
                    }
                    let point: Point = unsafe {
                        let __boltffi_value =
                            ::core::ptr::read_unaligned(
                                point as *const <Point as ::boltffi::__private::Passable>::In
                            );
                        <Point as ::boltffi::__private::Passable>::unpack(__boltffi_value)
                    };
                    norm(point)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn mutable_direct_record_param_expansion_passes_mutable_local() {
        let source = mutable_direct_record_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn shift(point: &mut Point) -> f64 {
                point.x += 1.0;
                point.x
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn shift(point: &mut Point) -> f64 {
                    point.x += 1.0;
                    point.x
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_shift(
                    point: <Point as ::boltffi::__private::Passable>::In
                ) -> f64 {
                    let mut point: Point = unsafe {
                        <Point as ::boltffi::__private::Passable>::unpack(point)
                    };
                    shift(&mut point)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_mutable_direct_record_param_expansion_writes_mutated_value_back() {
        let source = mutable_direct_record_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn shift(point: &mut Point) -> f64 {
                point.x += 1.0;
                point.x
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn shift(point: &mut Point) -> f64 {
                    point.x += 1.0;
                    point.x
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_shift(
                    point: *mut u8
                ) -> f64 {
                    let __boltffi_point_out = point;
                    if __boltffi_point_out.is_null() {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: null direct record pointer",
                            stringify!(point)
                        ));
                        return ::core::default::Default::default();
                    }
                    let mut point: Point = unsafe {
                        let __boltffi_value =
                            ::core::ptr::read_unaligned(
                                __boltffi_point_out as *const <Point as ::boltffi::__private::Passable>::In
                            );
                        <Point as ::boltffi::__private::Passable>::unpack(__boltffi_value)
                    };
                    let __boltffi_result = shift(&mut point);
                    unsafe {
                        ::core::ptr::write_unaligned(
                            __boltffi_point_out as *mut <Point as ::boltffi::__private::Passable>::In,
                            ::boltffi::__private::Passable::pack(point)
                        );
                    }
                    __boltffi_result
                }
            }
            .to_string()
        );
    }

    #[test]
    fn mutable_direct_primitive_param_expansion_passes_mutable_local() {
        let source = mutable_direct_primitive_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn bump(count: &mut i32) {
                *count += 1;
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn bump(count: &mut i32) {
                    *count += 1;
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_bump(
                    count: i32
                ) -> ::boltffi::__private::FfiStatus {
                    let mut count = count;
                    bump(&mut count);
                    ::boltffi::__private::FfiStatus::OK
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_string_param_expansion_decodes_owned_string() {
        let source = string_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn name_len(name: String) -> u32 {
                name.len() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

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
                    let name: String = {
                        if __boltffi_name_ptr.is_null() && __boltffi_name_len > 0 {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: null pointer with non-zero length (buf_len={})",
                                stringify!(name),
                                __boltffi_name_len
                            ));
                            return ::core::default::Default::default();
                        }
                        let __boltffi_bytes: &[u8] = if __boltffi_name_len == 0 {
                            &[]
                        } else {
                            unsafe {
                                ::core::slice::from_raw_parts(
                                    __boltffi_name_ptr,
                                    __boltffi_name_len
                                )
                            }
                        };
                        match ::boltffi::__private::wire::decode::<String>(__boltffi_bytes) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: wire decode failed: {} (buf_len={})",
                                    stringify!(name),
                                    error,
                                    __boltffi_name_len
                                ));
                                return ::core::default::Default::default();
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
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn name_len(name: &str) -> u32 {
                name.len() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

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
                    let __boltffi_name_storage: String = {
                        if __boltffi_name_ptr.is_null() && __boltffi_name_len > 0 {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: null pointer with non-zero length (buf_len={})",
                                stringify!(__boltffi_name_storage),
                                __boltffi_name_len
                            ));
                            return ::core::default::Default::default();
                        }
                        let __boltffi_bytes: &[u8] = if __boltffi_name_len == 0 {
                            &[]
                        } else {
                            unsafe {
                                ::core::slice::from_raw_parts(
                                    __boltffi_name_ptr,
                                    __boltffi_name_len
                                )
                            }
                        };
                        match ::boltffi::__private::wire::decode::<String>(__boltffi_bytes) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: wire decode failed: {} (buf_len={})",
                                    stringify!(__boltffi_name_storage),
                                    error,
                                    __boltffi_name_len
                                ));
                                return ::core::default::Default::default();
                            }
                        }
                    };
                    let name = __boltffi_name_storage.as_str();
                    name_len(name)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_mutable_string_param_expansion_decodes_mut_str_ref() {
        let source = mutable_string_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn rewrite(name: &mut str) -> u32 {
                name.len() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn rewrite(name: &mut str) -> u32 {
                    name.len() as u32
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_rewrite(
                    __boltffi_name_ptr: *const u8,
                    __boltffi_name_len: usize
                ) -> u32 {
                    let mut __boltffi_name_storage: String = {
                        if __boltffi_name_ptr.is_null() && __boltffi_name_len > 0 {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: null pointer with non-zero length (buf_len={})",
                                stringify!(__boltffi_name_storage),
                                __boltffi_name_len
                            ));
                            return ::core::default::Default::default();
                        }
                        let __boltffi_bytes: &[u8] = if __boltffi_name_len == 0 {
                            &[]
                        } else {
                            unsafe {
                                ::core::slice::from_raw_parts(
                                    __boltffi_name_ptr,
                                    __boltffi_name_len
                                )
                            }
                        };
                        match ::boltffi::__private::wire::decode::<String>(__boltffi_bytes) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: wire decode failed: {} (buf_len={})",
                                    stringify!(__boltffi_name_storage),
                                    error,
                                    __boltffi_name_len
                                ));
                                return ::core::default::Default::default();
                            }
                        }
                    };
                    let name = __boltffi_name_storage.as_mut_str();
                    rewrite(name)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_bytes_param_expansion_decodes_owned_bytes() {
        let source = bytes_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn bytes_len(bytes: Vec<u8>) -> u32 {
                bytes.len() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

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
                    let bytes: Vec<u8> = {
                        if __boltffi_bytes_ptr.is_null() && __boltffi_bytes_len > 0 {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: null pointer with non-zero length (buf_len={})",
                                stringify!(bytes),
                                __boltffi_bytes_len
                            ));
                            return ::core::default::Default::default();
                        }
                        let __boltffi_bytes: &[u8] = if __boltffi_bytes_len == 0 {
                            &[]
                        } else {
                            unsafe {
                                ::core::slice::from_raw_parts(
                                    __boltffi_bytes_ptr,
                                    __boltffi_bytes_len
                                )
                            }
                        };
                        match ::boltffi::__private::wire::decode::<Vec<u8> >(__boltffi_bytes) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: wire decode failed: {} (buf_len={})",
                                    stringify!(bytes),
                                    error,
                                    __boltffi_bytes_len
                                ));
                                return ::core::default::Default::default();
                            }
                        }
                    };
                    bytes_len(bytes)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_mutable_bytes_param_expansion_decodes_mut_slice_ref() {
        let source = mutable_bytes_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn fill(bytes: &mut [u8]) -> u32 {
                bytes.len() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn fill(bytes: &mut [u8]) -> u32 {
                    bytes.len() as u32
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_fill(
                    __boltffi_bytes_ptr: *const u8,
                    __boltffi_bytes_len: usize
                ) -> u32 {
                    let mut __boltffi_bytes_storage: Vec<u8> = {
                        if __boltffi_bytes_ptr.is_null() && __boltffi_bytes_len > 0 {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: null pointer with non-zero length (buf_len={})",
                                stringify!(__boltffi_bytes_storage),
                                __boltffi_bytes_len
                            ));
                            return ::core::default::Default::default();
                        }
                        let __boltffi_bytes: &[u8] = if __boltffi_bytes_len == 0 {
                            &[]
                        } else {
                            unsafe {
                                ::core::slice::from_raw_parts(
                                    __boltffi_bytes_ptr,
                                    __boltffi_bytes_len
                                )
                            }
                        };
                        match ::boltffi::__private::wire::decode::<Vec<u8> >(__boltffi_bytes) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: wire decode failed: {} (buf_len={})",
                                    stringify!(__boltffi_bytes_storage),
                                    error,
                                    __boltffi_bytes_len
                                ));
                                return ::core::default::Default::default();
                            }
                        }
                    };
                    let bytes = __boltffi_bytes_storage.as_mut_slice();
                    fill(bytes)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_option_i32_param_expansion_decodes_wire_option() {
        let source = option_i32_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn set_count(count: Option<i32>) {}
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn set_count(count: Option<i32>) {}
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_set_count(
                    __boltffi_count_ptr: *const u8,
                    __boltffi_count_len: usize
                ) -> ::boltffi::__private::FfiStatus {
                    let count: Option<i32> = if __boltffi_count_ptr.is_null() {
                        None
                    } else {
                        match ::boltffi::__private::wire::decode(unsafe {
                            ::core::slice::from_raw_parts(
                                __boltffi_count_ptr,
                                __boltffi_count_len
                            )
                        }) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: invalid optional scalar payload: {} (buf_len={})",
                                    stringify!(count),
                                    error,
                                    __boltffi_count_len
                                ));
                                return ::boltffi::__private::FfiStatus::INVALID_ARG;
                            }
                        }
                    };
                    set_count(count);
                    ::boltffi::__private::FfiStatus::OK
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_encoded_record_param_expansion_returns_on_decode_failure() {
        let source = encoded_record_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn name_score(profile: Profile) -> u32 {
                profile.name.len() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn name_score(profile: Profile) -> u32 {
                    profile.name.len() as u32
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_name_score(
                    __boltffi_profile_ptr: *const u8,
                    __boltffi_profile_len: usize
                ) -> u32 {
                    let profile: Profile = {
                        if __boltffi_profile_ptr.is_null() && __boltffi_profile_len > 0 {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: null pointer with non-zero length (buf_len={})",
                                stringify!(profile),
                                __boltffi_profile_len
                            ));
                            return ::core::default::Default::default();
                        }
                        let __boltffi_bytes: &[u8] = if __boltffi_profile_len == 0 {
                            &[]
                        } else {
                            unsafe {
                                ::core::slice::from_raw_parts(
                                    __boltffi_profile_ptr,
                                    __boltffi_profile_len
                                )
                            }
                        };
                        match ::boltffi::__private::wire::decode::<Profile>(__boltffi_bytes) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: wire decode failed: {} (buf_len={})",
                                    stringify!(profile),
                                    error,
                                    __boltffi_profile_len
                                ));
                                return ::core::default::Default::default();
                            }
                        }
                    };
                    name_score(profile)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_mutable_encoded_record_param_expansion_passes_mutable_storage_ref() {
        let source = mutable_encoded_record_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn rename(profile: &mut Profile) -> u32 {
                profile.name.len() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn rename(profile: &mut Profile) -> u32 {
                    profile.name.len() as u32
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_rename(
                    __boltffi_profile_ptr: *const u8,
                    __boltffi_profile_len: usize
                ) -> u32 {
                    let mut __boltffi_profile_storage: Profile = {
                        if __boltffi_profile_ptr.is_null() && __boltffi_profile_len > 0 {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: null pointer with non-zero length (buf_len={})",
                                stringify!(__boltffi_profile_storage),
                                __boltffi_profile_len
                            ));
                            return ::core::default::Default::default();
                        }
                        let __boltffi_bytes: &[u8] = if __boltffi_profile_len == 0 {
                            &[]
                        } else {
                            unsafe {
                                ::core::slice::from_raw_parts(
                                    __boltffi_profile_ptr,
                                    __boltffi_profile_len
                                )
                            }
                        };
                        match ::boltffi::__private::wire::decode::<Profile>(__boltffi_bytes) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: wire decode failed: {} (buf_len={})",
                                    stringify!(__boltffi_profile_storage),
                                    error,
                                    __boltffi_profile_len
                                ));
                                return ::core::default::Default::default();
                            }
                        }
                    };
                    let profile = &mut __boltffi_profile_storage;
                    rename(profile)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_class_param_expansion_consumes_required_handle_and_returns_nullable_handle() {
        let source = class_param_nullable_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn open(engine: Engine) -> Option<Engine> {
                Some(engine)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn open(engine: Engine) -> Option<Engine> {
                    Some(engine)
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_open(
                    engine: u64
                ) -> u64 {
                    if engine == 0 {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: null class handle",
                            stringify!(engine)
                        ));
                        return 0;
                    }
                    let engine: Engine = unsafe {
                        *Box::from_raw(engine as usize as *mut Engine)
                    };
                    let __boltffi_result: Option<Engine> = open(engine);
                    match __boltffi_result {
                        Some(__boltffi_value) => {
                            Box::into_raw(Box::new(__boltffi_value)) as usize as u64
                        }
                        None => 0,
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_boxed_callback_param_expansion_recovers_boxed_trait_object() {
        let source = boxed_callback_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn listen(listener: Box<dyn Listener>) {}
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn listen(listener: Box<dyn Listener>) {}
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_listen(
                    listener: ::boltffi::__private::CallbackHandle
                ) -> ::boltffi::__private::FfiStatus {
                    let __boltffi_listener_handle = listener;
                    if __boltffi_listener_handle.is_null() {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: null callback handle",
                            stringify!(listener)
                        ));
                        return ::boltffi::__private::FfiStatus::INVALID_ARG;
                    }
                    let listener: Box<dyn Listener> = unsafe {
                        <dyn Listener as ::boltffi::__private::BoxFromCallbackHandle>::box_from_callback_handle(
                            __boltffi_listener_handle
                        )
                    };
                    listen(listener);
                    ::boltffi::__private::FfiStatus::OK
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_nullable_arc_callback_param_expansion_recovers_optional_shared_trait_object() {
        let source = nullable_arc_callback_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn maybe(listener: Option<std::sync::Arc<dyn Listener>>) -> u32 {
                listener.is_some() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn maybe(listener: Option<std::sync::Arc<dyn Listener> >) -> u32 {
                    listener.is_some() as u32
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_maybe(
                    listener: u32
                ) -> u32 {
                    let __boltffi_listener_handle =
                        ::boltffi::__private::CallbackHandle::from_wasm_handle(listener);
                    let listener: Option<::std::sync::Arc<dyn Listener> > =
                        if __boltffi_listener_handle.is_null() {
                            None
                        } else {
                            Some(unsafe {
                                <dyn Listener as ::boltffi::__private::ArcFromCallbackHandle>::arc_from_callback_handle(
                                    __boltffi_listener_handle
                                )
                            })
                        };
                    maybe(listener)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_closure_param_expansion_builds_invoke_context_closure() {
        let source = closure_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn apply(callback: impl Fn(u32) -> u32) -> u32 {
                callback(41)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn apply(callback: impl Fn(u32) -> u32) -> u32 {
                    callback(41)
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_apply(
                    __boltffi_callback_call: extern "C" fn(*mut ::core::ffi::c_void, u32) -> u32,
                    __boltffi_callback_context: *mut ::core::ffi::c_void,
                    __boltffi_callback_release: unsafe extern "C" fn(*mut ::core::ffi::c_void)
                ) -> u32 {
                    let __boltffi_callback_owner =
                        ::boltffi::__private::NativeCallbackOwner::new(
                            __boltffi_callback_context,
                            __boltffi_callback_release
                        );
                    let callback = move |__boltffi_arg0: u32| {
                        unsafe {
                            __boltffi_callback_call(
                                __boltffi_callback_owner.context(),
                                __boltffi_arg0
                            )
                        }
                    };
                    apply(callback)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_closure_param_expansion_builds_import_backed_closure() {
        let source = closure_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn apply(callback: impl Fn(u32) -> u32) -> u32 {
                callback(41)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn apply(callback: impl Fn(u32) -> u32) -> u32 {
                    callback(41)
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_apply(callback: u32) -> u32 {
                    unsafe extern "C" {
                        fn __boltffi_callback_closure____closure__u32_to_u32_call(
                            handle: u32,
                            __boltffi_ffi_arg0: u32
                        ) -> u32;
                        fn __boltffi_callback_closure____closure__u32_to_u32_free(handle: u32);
                    }
                    if callback == 0 {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: null closure handle",
                            stringify!(callback)
                        ));
                        return ::core::default::Default::default();
                    }
                    let __boltffi_callback_owner =
                        ::boltffi::__private::WasmCallbackOwner::new(
                            callback,
                            __boltffi_callback_closure____closure__u32_to_u32_free
                        );
                    let callback = move |__boltffi_arg0: u32| {
                        unsafe {
                            __boltffi_callback_closure____closure__u32_to_u32_call(
                                __boltffi_callback_owner.handle(),
                                __boltffi_arg0
                            )
                        }
                    };
                    apply(callback)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_closure_param_expansion_decodes_encoded_invoke_return() {
        let source = string_closure_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn apply(callback: impl Fn(String) -> String) -> u32 {
                callback("x".to_string()).len() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn apply(callback: impl Fn(String) -> String) -> u32 {
                    callback("x".to_string()).len() as u32
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_apply(
                    __boltffi_callback_call: extern "C" fn(
                        *mut ::core::ffi::c_void,
                        *const u8,
                        usize
                    ) -> ::boltffi::__private::FfiBuf,
                    __boltffi_callback_context: *mut ::core::ffi::c_void,
                    __boltffi_callback_release: unsafe extern "C" fn(*mut ::core::ffi::c_void)
                ) -> u32 {
                    let __boltffi_callback_owner =
                        ::boltffi::__private::NativeCallbackOwner::new(
                            __boltffi_callback_context,
                            __boltffi_callback_release
                        );
                    let callback = move |__boltffi_arg0: String| {
                        {
                            let __boltffi_result_buf = unsafe {
                                let __boltffi_arg0_wire =
                                    ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_arg0);
                                let __boltffi_arg0_ptr = __boltffi_arg0_wire.as_ptr();
                                let __boltffi_arg0_len = __boltffi_arg0_wire.len();
                                __boltffi_callback_call(
                                    __boltffi_callback_owner.context(),
                                    __boltffi_arg0_ptr,
                                    __boltffi_arg0_len
                                )
                            };
                            let __boltffi_result_bytes = unsafe {
                                __boltffi_result_buf.as_byte_slice()
                            };
                            ::boltffi::__private::wire::decode::<String>(
                                __boltffi_result_bytes
                            ).expect("closure return: wire decode failed")
                        }
                    };
                    apply(callback)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_closure_param_expansion_decodes_packed_string_invoke_return() {
        let source = string_closure_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn apply(callback: impl Fn(String) -> String) -> u32 {
                callback("x".to_string()).len() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn apply(callback: impl Fn(String) -> String) -> u32 {
                    callback("x".to_string()).len() as u32
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_apply(callback: u32) -> u32 {
                    unsafe extern "C" {
                        fn __boltffi_callback_closure____closure__string_to_string_call(
                            handle: u32,
                            __boltffi_ffi_arg0: *const u8,
                            __boltffi_ffi_arg1: usize
                        ) -> u64;
                        fn __boltffi_callback_closure____closure__string_to_string_free(handle: u32);
                    }
                    if callback == 0 {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: null closure handle",
                            stringify!(callback)
                        ));
                        return ::core::default::Default::default();
                    }
                    let __boltffi_callback_owner =
                        ::boltffi::__private::WasmCallbackOwner::new(
                            callback,
                            __boltffi_callback_closure____closure__string_to_string_free
                        );
                    let callback = move |__boltffi_arg0: String| {
                        {
                            let __boltffi_result_packed = unsafe {
                                let __boltffi_arg0_wire =
                                    ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_arg0);
                                let __boltffi_arg0_ptr = __boltffi_arg0_wire.as_ptr();
                                let __boltffi_arg0_len = __boltffi_arg0_wire.len();
                                __boltffi_callback_closure____closure__string_to_string_call(
                                    __boltffi_callback_owner.handle(),
                                    __boltffi_arg0_ptr,
                                    __boltffi_arg0_len
                                )
                            };
                            unsafe {
                                ::boltffi::__private::take_packed_utf8_string(
                                    __boltffi_result_packed
                                )
                            }
                        }
                    };
                    apply(callback)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_closure_param_expansion_returns_fallible_void_result() {
        let source = fallible_closure_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn apply(callback: impl Fn() -> Result<(), String>) {
                let _ = callback();
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded fallible closure param parses");

        assert!(
            rendered.contains("__boltffi_callback_call (__boltffi_callback_owner . context ())")
        );
        assert!(rendered.contains("if __boltffi_error_buf . is_empty () { Ok (()) }"));
        assert!(rendered.contains(
            "Err (:: boltffi :: __private :: wire :: decode :: < String > (__boltffi_error_bytes)"
        ));
    }

    #[test]
    fn wasm_closure_param_expansion_returns_fallible_void_result() {
        let source = fallible_closure_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn apply(callback: impl Fn() -> Result<(), String>) {
                let _ = callback();
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded fallible closure param parses");

        assert!(rendered.contains(
            "__boltffi_callback_closure____closure__to_result_void_err_string_call (__boltffi_callback_owner . handle ())"
        ));
        assert!(rendered.contains("if __boltffi_error_packed == 0 { Ok (()) }"));
        assert!(rendered.contains(
            ":: boltffi :: __private :: take_packed_utf8_string (__boltffi_error_packed)"
        ));
    }

    #[test]
    fn native_closure_param_expansion_writes_fallible_direct_success_out_pointer() {
        let source = fallible_i32_closure_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn apply(callback: impl Fn() -> Result<i32, String>) {
                let _ = callback();
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded fallible closure param parses");

        assert!(rendered.contains("extern \"C\" fn (* mut :: core :: ffi :: c_void , * mut i32) -> :: boltffi :: __private :: FfiBuf"));
        assert!(rendered.contains("MaybeUninit :: < i32 > :: uninit ()"));
        assert!(rendered.contains("Ok (unsafe { __boltffi_success . assume_init () })"));
        assert!(rendered.contains("Err (:: boltffi :: __private :: wire :: decode :: < String >"));
    }

    #[test]
    fn wasm_closure_param_expansion_writes_fallible_encoded_success_out_pointer() {
        let source = fallible_string_closure_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn apply(callback: impl Fn() -> Result<String, String>) {
                let _ = callback();
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        let rendered = tokens.to_string();
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded fallible closure param parses");

        assert!(rendered.contains("fn __boltffi_callback_closure____closure__to_result_string_err_string_call (handle : u32 , __boltffi_ffi_arg0 : * mut u64) -> u64"));
        assert!(rendered.contains("MaybeUninit :: < u64 > :: uninit ()"));
        assert!(rendered.contains("take_packed_utf8_string (__boltffi_success . assume_init ())"));
        assert!(rendered.contains("take_packed_utf8_string (__boltffi_error_packed)"));
    }

    #[test]
    fn native_closure_return_expansion_writes_owned_context_out_pointer() {
        let source = closure_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_callback() -> impl Fn(u32) -> u32 {
                |value| value + 1
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded closure return parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "pub unsafe extern \"C\" fn boltffi_function_demo_make_callback (__boltffi_return_out : * mut :: core :: ffi :: c_void) -> :: boltffi :: __private :: FfiStatus"
        ));
        assert!(rendered.contains(
            "struct __BoltffiClosureReturn__boltffi_closure { invoke : Option < unsafe extern \"C\" fn (* mut :: core :: ffi :: c_void , u32) -> u32 > , context : * mut :: core :: ffi :: c_void , release : Option < unsafe extern \"C\" fn (* mut :: core :: ffi :: c_void) > , }"
        ));
        assert!(rendered.contains(
            "* __boltffi_return_out = __BoltffiClosureReturn__boltffi_closure { invoke : Some (__boltffi_make_callback_closure_call) , context : __boltffi_closure_context , release : Some (__boltffi_make_callback_closure_release) , }"
        ));
    }

    #[test]
    fn wasm_closure_return_expansion_exports_call_free_and_writes_handle() {
        let source = closure_return_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_callback() -> impl Fn(u32) -> u32 {
                |value| value + 1
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded closure return parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "pub unsafe extern \"C\" fn boltffi_function_demo_make_callback (__boltffi_return_out : * mut u32) -> :: boltffi :: __private :: FfiStatus"
        ));
        assert!(rendered.contains(
            "# [cfg (target_arch = \"wasm32\")] # [unsafe (no_mangle)] pub unsafe extern \"C\" fn boltffi_closure_1____closure__u32_to_u32_call (__boltffi_context : u32 , __boltffi_arg0 : u32) -> u32"
        ));
        assert!(rendered.contains(
            "# [cfg (target_arch = \"wasm32\")] # [unsafe (no_mangle)] pub unsafe extern \"C\" fn boltffi_closure_1____closure__u32_to_u32_free (__boltffi_context : u32)"
        ));
        assert!(rendered.contains(
            "* __boltffi_return_out = Box :: into_raw (Box :: new (Box :: new (__boltffi_closure) as Box < dyn Fn (u32) -> u32 + 'static >)) as usize as u32"
        ));
    }

    #[test]
    fn native_function_pointer_closure_return_expansion_boxes_pointer_context() {
        let source = function_pointer_closure_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_callback() -> fn(u32) -> u32 {
                fn increment(value: u32) -> u32 {
                    value + 1
                }
                increment
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded function-pointer closure parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "unsafe extern \"C\" fn __boltffi_make_callback_closure_call (__boltffi_context : * mut :: core :: ffi :: c_void , __boltffi_arg0 : u32) -> u32"
        ));
        assert!(
            rendered.contains(
                "Box :: new (__boltffi_closure) as Box < dyn Fn (u32) -> u32 + 'static >"
            )
        );
        assert!(rendered.contains(
            "struct __BoltffiClosureReturn__boltffi_closure { invoke : Option < unsafe extern \"C\" fn (* mut :: core :: ffi :: c_void , u32) -> u32 > , context : * mut :: core :: ffi :: c_void , release : Option < unsafe extern \"C\" fn (* mut :: core :: ffi :: c_void) > , }"
        ));
    }

    #[test]
    fn wasm_function_pointer_closure_return_expansion_boxes_pointer_context() {
        let source = function_pointer_closure_return_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_callback() -> fn(u32) -> u32 {
                fn increment(value: u32) -> u32 {
                    value + 1
                }
                increment
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded function-pointer closure parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "# [cfg (target_arch = \"wasm32\")] # [unsafe (no_mangle)] pub unsafe extern \"C\" fn boltffi_closure_1____closure__u32_to_u32_call (__boltffi_context : u32 , __boltffi_arg0 : u32) -> u32"
        ));
        assert!(rendered.contains(
            "* __boltffi_return_out = Box :: into_raw (Box :: new (Box :: new (__boltffi_closure) as Box < dyn Fn (u32) -> u32 + 'static >)) as usize as u32"
        ));
    }

    #[test]
    fn native_closure_return_invoke_accepts_closure_parameter() {
        let source = closure_return_with_closure_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_runner() -> impl Fn(Box<dyn Fn(u32) -> u32>) -> u32 {
                |callback| callback(41)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded nested closure return parses");
        let rendered = tokens.to_string();
        println!("{rendered}");

        assert!(rendered.contains(
            "unsafe extern \"C\" fn __boltffi_make_runner_closure_call (__boltffi_context : * mut :: core :: ffi :: c_void , __boltffi_arg0_call : extern \"C\" fn (* mut :: core :: ffi :: c_void , u32) -> u32 , __boltffi_arg0_context : * mut :: core :: ffi :: c_void , __boltffi_arg0_release : unsafe extern \"C\" fn (* mut :: core :: ffi :: c_void)) -> u32"
        ));
        assert!(rendered.contains(
            "let __boltffi_arg0 : Box < dyn Fn (u32) -> u32 > = Box :: new (move | __boltffi_arg0 : u32 |"
        ));
        assert!(
            rendered.contains(
                "__boltffi_arg0_call (__boltffi_arg0_owner . context () , __boltffi_arg0)"
            )
        );
    }

    #[test]
    fn wasm_closure_return_invoke_accepts_closure_parameter() {
        let source = closure_return_with_closure_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_runner() -> impl Fn(Box<dyn Fn(u32) -> u32>) -> u32 {
                |callback| callback(41)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded nested closure return parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "pub unsafe extern \"C\" fn boltffi_closure_1____closure__box_closure_to_u32_call (__boltffi_context : u32 , __boltffi_arg0 : u32) -> u32"
        ));
        assert!(rendered.contains(
            "unsafe extern \"C\" { fn __boltffi_callback_closure____closure__u32_to_u32_call (handle : u32 , __boltffi_ffi_arg0 : u32) -> u32 ; fn __boltffi_callback_closure____closure__u32_to_u32_free (handle : u32) ; }"
        ));
        assert!(rendered.contains(
            "__boltffi_callback_closure____closure__u32_to_u32_call (__boltffi_arg0_owner . handle () , __boltffi_arg0)"
        ));
    }

    #[test]
    fn native_encoded_closure_return_invoke_decodes_arg_and_returns_buffer() {
        let source = string_closure_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_mapper() -> impl Fn(String) -> String {
                |value| value
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded encoded closure return parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "unsafe extern \"C\" fn __boltffi_make_mapper_closure_call (__boltffi_context : * mut :: core :: ffi :: c_void , __boltffi_arg0_ptr : * const u8 , __boltffi_arg0_len : usize) -> :: boltffi :: __private :: FfiBuf"
        ));
        assert!(rendered.contains("let __boltffi_arg0 : String = {"));
        assert!(
            rendered.contains(
                ":: boltffi :: __private :: wire :: decode :: < String > (__boltffi_bytes)"
            )
        );
        assert!(
            rendered
                .contains(":: boltffi :: __private :: FfiBuf :: wire_encode (& __boltffi_result)")
        );
    }

    #[test]
    fn wasm_encoded_closure_return_invoke_decodes_arg_and_returns_packed_buffer() {
        let source = string_closure_return_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_mapper() -> impl Fn(String) -> String {
                |value| value
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded encoded closure return parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "# [cfg (target_arch = \"wasm32\")] # [unsafe (no_mangle)] pub unsafe extern \"C\" fn boltffi_closure_1____closure__string_to_string_call (__boltffi_context : u32 , __boltffi_arg0_ptr : * const u8 , __boltffi_arg0_len : usize) -> u64"
        ));
        assert!(rendered.contains("let __boltffi_arg0 : String = {"));
        assert!(
            rendered.contains(
                ":: boltffi :: __private :: wire :: decode :: < String > (__boltffi_bytes)"
            )
        );
        assert!(rendered.contains(
            ":: boltffi :: __private :: FfiBuf :: wire_encode (& __boltffi_result) . into_packed ()"
        ));
    }

    #[test]
    fn native_result_closure_return_expansion_writes_closure_success_and_encoded_error() {
        let source = result_closure_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn try_make_callback() -> Result<Box<dyn Fn(u32) -> u32>, String> {
                Ok(Box::new(|value| value + 1))
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded closure result parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "pub unsafe extern \"C\" fn boltffi_function_demo_try_make_callback (__boltffi_return_out : * mut :: core :: ffi :: c_void) -> :: boltffi :: __private :: FfiBuf"
        ));
        assert!(rendered.contains(
            "Ok (__boltffi_success) => { # [repr (C)] struct __BoltffiClosureReturn__boltffi_success"
        ));
        assert!(
            rendered.contains("invoke : Some (__boltffi_try_make_callback_success_closure_call)")
        );
        assert!(
            rendered
                .contains("release : Some (__boltffi_try_make_callback_success_closure_release)")
        );
        assert!(rendered.contains(
            "Err (__boltffi_error) => { :: boltffi :: __private :: FfiBuf :: wire_encode (& __boltffi_error) }"
        ));
        assert!(rendered.contains(":: boltffi :: __private :: FfiBuf :: default ()"));
    }

    #[test]
    fn native_closure_return_invoke_writes_fallible_direct_success_out_pointer() {
        let source = fallible_i32_closure_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_callback() -> impl Fn() -> Result<i32, String> {
                || Ok(7)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded fallible closure invoke parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "unsafe extern \"C\" fn __boltffi_make_callback_closure_call (__boltffi_context : * mut :: core :: ffi :: c_void , __boltffi_success_out : * mut i32) -> :: boltffi :: __private :: FfiBuf"
        ));
        assert!(rendered.contains("match __boltffi_closure ()"));
        assert!(rendered.contains("* __boltffi_success_out = __boltffi_success"));
        assert!(rendered.contains(":: boltffi :: __private :: FfiBuf :: default ()"));
        assert!(rendered.contains(
            "Err (__boltffi_error) => { :: boltffi :: __private :: FfiBuf :: wire_encode (& __boltffi_error) }"
        ));
    }

    #[test]
    fn wasm_closure_return_invoke_writes_fallible_encoded_success_out_pointer() {
        let source = fallible_string_closure_return_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn make_mapper() -> impl Fn() -> Result<String, String> {
                || Ok("x".to_string())
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");
        syn::parse2::<syn::File>(tokens.clone()).expect("expanded fallible closure invoke parses");
        let rendered = tokens.to_string();

        assert!(rendered.contains(
            "# [cfg (target_arch = \"wasm32\")] # [unsafe (no_mangle)] pub unsafe extern \"C\" fn boltffi_closure_1____closure__to_result_string_err_string_call (__boltffi_context : u32 , __boltffi_success_out : * mut u64) -> u64"
        ));
        assert!(rendered.contains("match __boltffi_closure ()"));
        assert!(rendered.contains(
            "* __boltffi_success_out = :: boltffi :: __private :: FfiBuf :: wire_encode (& __boltffi_success) . into_packed ()"
        ));
        assert!(rendered.contains(
            "Err (__boltffi_error) => { :: boltffi :: __private :: FfiBuf :: wire_encode (& __boltffi_error) . into_packed () }"
        ));
    }

    #[test]
    fn wasm_borrowed_class_param_expansion_borrows_required_handle() {
        let source = borrowed_class_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn engine_id(engine: &Engine) -> u32 {
                7
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn engine_id(engine: &Engine) -> u32 {
                    7
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_engine_id(
                    engine: u32
                ) -> u32 {
                    if engine == 0 {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: null class handle",
                            stringify!(engine)
                        ));
                        return ::core::default::Default::default();
                    }
                    let engine: &Engine = unsafe {
                        &*(engine as usize as *const Engine)
                    };
                    engine_id(engine)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_result_class_string_expansion_writes_success_handle_out_pointer() {
        let source = result_class_string_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn try_open() -> Result<Engine, String> {
                Ok(Engine)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn try_open() -> Result<Engine, String> {
                    Ok(Engine)
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_try_open(
                    __boltffi_return_out: *mut u64
                ) -> ::boltffi::__private::FfiBuf {
                    match try_open() {
                        Ok(__boltffi_success) => {
                            if !__boltffi_return_out.is_null() {
                                unsafe {
                                    *__boltffi_return_out =
                                        Box::into_raw(Box::new(__boltffi_success)) as usize as u64;
                                }
                            }
                            ::boltffi::__private::FfiBuf::default()
                        }
                        Err(__boltffi_error) => {
                            ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_error)
                        }
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_option_i32_param_expansion_decodes_nan_sentinel_scalar() {
        let source = option_i32_param_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn set_count(count: Option<i32>) {}
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn set_count(count: Option<i32>) {}
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_set_count(
                    count: f64
                ) -> ::boltffi::__private::FfiStatus {
                    let count: Option<i32> = if count.is_nan() {
                        None
                    } else {
                        Some(count as _)
                    };
                    set_count(count);
                    ::boltffi::__private::FfiStatus::OK
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_vec_u32_param_expansion_decodes_direct_vector() {
        let source = vec_u32_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn sum(values: Vec<u32>) -> u32 {
                values.into_iter().sum()
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn sum(values: Vec<u32>) -> u32 {
                    values.into_iter().sum()
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_sum(
                    __boltffi_values_ptr: *const u32,
                    __boltffi_values_len: usize
                ) -> u32 {
                    let values: Vec<u32> = if __boltffi_values_ptr.is_null() {
                        Vec::new()
                    } else {
                        unsafe {
                            ::core::slice::from_raw_parts(
                                __boltffi_values_ptr,
                                __boltffi_values_len
                            )
                        }.to_vec()
                    };
                    sum(values)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_vec_record_param_expansion_decodes_passable_vector() {
        let source = vec_point_param_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn count_points(points: Vec<Point>) -> u32 {
                points.len() as u32
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn count_points(points: Vec<Point>) -> u32 {
                    points.len() as u32
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_count_points(
                    __boltffi_points_ptr: *const u8,
                    __boltffi_points_len: usize
                ) -> u32 {
                    let points: Vec<Point> = if __boltffi_points_ptr.is_null() {
                        Vec::new()
                    } else {
                        let raw_byte_len = __boltffi_points_len;
                        let element_size =
                            ::core::mem::size_of::<<Point as ::boltffi::__private::Passable>::In>();
                        if raw_byte_len % element_size == 0 {
                            unsafe {
                                <Point as ::boltffi::__private::VecTransport>::unpack_vec(
                                    __boltffi_points_ptr,
                                    raw_byte_len
                                )
                            }
                        } else {
                            ::boltffi::__private::set_last_error(format!(
                                "invalid byte length {} for Vec<{}>: not divisible by element size {}",
                                raw_byte_len,
                                ::core::any::type_name::<Point>(),
                                element_size
                            ));
                            return ::core::default::Default::default();
                        }
                    };
                    count_points(points)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn direct_record_return_expansion_packs_passable_output() {
        let source = direct_record_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn origin() -> Point {
                Point { x: 0.0 }
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

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
    fn native_result_i32_string_expansion_writes_success_out_pointer() {
        let source = result_i32_string_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn try_count() -> Result<i32, String> {
                Ok(7)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn try_count() -> Result<i32, String> {
                    Ok(7)
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_try_count(
                    __boltffi_return_out: *mut i32
                ) -> ::boltffi::__private::FfiBuf {
                    match try_count() {
                        Ok(__boltffi_success) => {
                            if !__boltffi_return_out.is_null() {
                                unsafe {
                                    *__boltffi_return_out = __boltffi_success;
                                }
                            }
                            ::boltffi::__private::FfiBuf::default()
                        }
                        Err(__boltffi_error) => {
                            ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_error)
                        }
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_result_unit_string_expansion_returns_encoded_error_status() {
        let source = result_unit_string_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn try_ping() -> Result<(), String> {
                Ok(())
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn try_ping() -> Result<(), String> {
                    Ok(())
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_try_ping() -> u64 {
                    match try_ping() {
                        Ok(()) => {
                            ::boltffi::__private::FfiBuf::default().into_packed()
                        }
                        Err(__boltffi_error) => {
                            ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_error).into_packed()
                        }
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_result_string_string_expansion_writes_encoded_success_out_pointer() {
        let source = result_string_string_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn try_greet() -> Result<String, String> {
                Ok(String::from("hello"))
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn try_greet() -> Result<String, String> {
                    Ok(String::from("hello"))
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn boltffi_function_demo_try_greet(
                    __boltffi_return_out: *mut u64
                ) -> u64 {
                    match try_greet() {
                        Ok(__boltffi_success) => {
                            if !__boltffi_return_out.is_null() {
                                unsafe {
                                    *__boltffi_return_out =
                                        ::boltffi::__private::FfiBuf::wire_encode(
                                            &__boltffi_success
                                        ).into_packed();
                                }
                            }
                            ::boltffi::__private::FfiBuf::default().into_packed()
                        }
                        Err(__boltffi_error) => {
                            ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_error).into_packed()
                        }
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_option_i32_return_expansion_returns_encoded_option_buffer() {
        let source = option_i32_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn maybe_count() -> Option<i32> {
                Some(7)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn maybe_count() -> Option<i32> {
                    Some(7)
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_maybe_count() -> ::boltffi::__private::FfiBuf {
                    let __boltffi_result: Option<i32> = maybe_count();
                    ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_result)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_option_i32_return_expansion_returns_nan_sentinel_scalar() {
        let source = option_i32_return_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn maybe_count() -> Option<i32> {
                Some(7)
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn maybe_count() -> Option<i32> {
                    Some(7)
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_maybe_count() -> f64 {
                    let __boltffi_result: Option<i32> = maybe_count();
                    match __boltffi_result {
                        Some(__boltffi_value) => __boltffi_value as f64,
                        None => f64::NAN,
                    }
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_vec_i32_return_expansion_returns_direct_vector_buffer() {
        let source = vec_i32_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn numbers() -> Vec<i32> {
                vec![1, 2, 3]
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn numbers() -> Vec<i32> {
                    vec![1, 2, 3]
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_numbers() -> ::boltffi::__private::FfiBuf {
                    let __boltffi_result = numbers();
                    <_ as ::boltffi::__private::VecTransport>::pack_vec(__boltffi_result)
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_vec_i32_return_expansion_writes_direct_vector_return_slot() {
        let source = vec_i32_return_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn numbers() -> Vec<i32> {
                vec![1, 2, 3]
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn numbers() -> Vec<i32> {
                    vec![1, 2, 3]
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_numbers() {
                    let __boltffi_result = numbers();
                    let __boltffi_buf = ::boltffi::__private::FfiBuf::from_vec(__boltffi_result);
                    ::boltffi::__private::write_return_slot(
                        __boltffi_buf.as_ptr() as u32,
                        __boltffi_buf.len() as u32,
                        __boltffi_buf.cap() as u32,
                        __boltffi_buf.align() as u32
                    );
                    core::mem::forget(__boltffi_buf);
                }
            }
            .to_string()
        );
    }

    #[test]
    fn native_string_return_expansion_returns_buffer() {
        let source = string_return_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn greet() -> String {
                String::from("hello")
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

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
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn greet() -> String {
                String::from("hello")
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

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
                    ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_result).into_packed()
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_bytes_return_expansion_uses_wire_framing() {
        let source = bytes_return_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&lowered);
        let syntax = syn::parse_quote! {
            pub fn payload() -> Vec<u8> {
                vec![1, 2, 3]
            }
        };

        let tokens =
            expand_function(&expansion, &source.functions[0], syntax).expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn payload() -> Vec<u8> {
                    vec![1, 2, 3]
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_payload() -> u64 {
                    let __boltffi_result: Vec<u8> = payload();
                    ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_result).into_packed()
                }
            }
            .to_string()
        );
    }
}

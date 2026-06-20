use std::fmt;

use boltffi_ast::{FnSig, Primitive as AstPrimitive, ReturnDef, TypeExpr};

use crate::{ImportModule, ImportSymbol, NativeSymbol, SymbolName, wasm32};

use super::{
    LowerError,
    symbol::{self, SymbolAllocator},
};

pub fn incoming_registration(
    closure: &FnSig,
) -> Result<wasm32::IncomingClosureRegistration, LowerError> {
    let module = ImportModule::parse(symbol::WASM_CALLBACK_IMPORT_MODULE.to_owned())?;
    let signature = ClosureSignature::from_closure(closure).symbol_part();
    let call = import_symbol(module.clone(), &signature, "call")?;
    let free = import_symbol(module, &signature, "free")?;
    Ok(wasm32::IncomingClosureRegistration::new(call, free))
}

pub fn outgoing_registration(
    allocator: &mut SymbolAllocator,
    closure: &FnSig,
) -> Result<wasm32::OutgoingClosureRegistration, LowerError> {
    let signature = ClosureSignature::from_closure(closure).symbol_part();
    let group_id = allocator.next_group_id();
    let call = export_symbol(allocator, group_id, &signature, "call")?;
    let free = export_symbol(allocator, group_id, &signature, "free")?;
    Ok(wasm32::OutgoingClosureRegistration::new(call, free))
}

fn import_symbol(
    module: ImportModule,
    signature: &str,
    action: &str,
) -> Result<ImportSymbol, LowerError> {
    let name = symbol::wasm_callback_import_name("closure", signature, action);
    Ok(ImportSymbol::new(module, SymbolName::parse(name)?))
}

fn export_symbol(
    allocator: &mut SymbolAllocator,
    group_id: u32,
    signature: &str,
    action: &str,
) -> Result<NativeSymbol, LowerError> {
    allocator.mint(symbol::wasm_closure_export_name(
        group_id, signature, action,
    ))
}

struct ClosureSignature<'signature> {
    params: &'signature [TypeExpr],
    returns: &'signature ReturnDef,
}

impl<'signature> ClosureSignature<'signature> {
    fn from_closure(closure: &'signature FnSig) -> Self {
        Self {
            params: &closure.parameters,
            returns: &closure.returns,
        }
    }

    fn symbol_part(&self) -> String {
        closure_symbol_case(&format!("__Closure_{self}"))
    }
}

impl<'signature> fmt::Display for ClosureSignature<'signature> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (
            self.params.is_empty(),
            return_signature_is_void(self.returns),
        ) {
            (true, true) => formatter.write_str("Void"),
            (false, true) => write_parameter_signature_types(formatter, self.params),
            (true, false) => {
                formatter.write_str("To")?;
                write_return_signature(formatter, self.returns)
            }
            (false, false) => {
                write_parameter_signature_types(formatter, self.params)?;
                formatter.write_str("To")?;
                write_return_signature(formatter, self.returns)
            }
        }
    }
}

fn return_signature_is_void(returns: &ReturnDef) -> bool {
    matches!(returns, ReturnDef::Void)
        || matches!(returns, ReturnDef::Value(type_expr) if matches!(type_expr, TypeExpr::Unit))
}

fn write_return_signature(formatter: &mut fmt::Formatter<'_>, returns: &ReturnDef) -> fmt::Result {
    match returns {
        ReturnDef::Void => formatter.write_str("Void"),
        ReturnDef::Value(type_expr) => {
            write!(formatter, "{}", ClosureTypeSignature(type_expr))
        }
    }
}

struct ClosureTypeSignature<'signature>(&'signature TypeExpr);

impl<'signature> fmt::Display for ClosureTypeSignature<'signature> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            TypeExpr::Primitive(primitive) => formatter.write_str(&primitive_signature(*primitive)),
            TypeExpr::Unit => formatter.write_str("Void"),
            TypeExpr::Record { id, .. } => formatter.write_str(&source_type_signature(id.as_str())),
            TypeExpr::Enum { id, .. } => formatter.write_str(&source_type_signature(id.as_str())),
            TypeExpr::Class { id, .. } => formatter.write_str(&source_type_signature(id.as_str())),
            TypeExpr::ImplTrait(bounds) | TypeExpr::Dyn(bounds) => match &bounds.base {
                boltffi_ast::BaseTrait::Named { id, .. } => {
                    formatter.write_str(&source_type_signature(id.as_str()))
                }
                boltffi_ast::BaseTrait::Function(_) => formatter.write_str("Closure"),
            },
            TypeExpr::FnPtr(_) => formatter.write_str("Closure"),
            TypeExpr::Custom { id, .. } => formatter.write_str(&source_type_signature(id.as_str())),
            TypeExpr::SelfType => formatter.write_str("Self"),
            TypeExpr::Vec(inner) => write!(formatter, "Vec{}", ClosureTypeSignature(inner)),
            TypeExpr::Slice(inner) => write!(formatter, "Slice{}", ClosureTypeSignature(inner)),
            TypeExpr::Boxed(inner) => write!(formatter, "Box{}", ClosureTypeSignature(inner)),
            TypeExpr::Arc(inner) => write!(formatter, "Arc{}", ClosureTypeSignature(inner)),
            TypeExpr::Option(inner) => write!(formatter, "Opt{}", ClosureTypeSignature(inner)),
            TypeExpr::Result { ok, err } => write!(
                formatter,
                "Result{}Err{}",
                ClosureTypeSignature(ok),
                ClosureTypeSignature(err)
            ),
            TypeExpr::Tuple(elements) => {
                formatter.write_str("Tuple")?;
                write_signature_types(formatter, elements)
            }
            TypeExpr::Map { key, value, .. } => write!(
                formatter,
                "Map{}To{}",
                ClosureTypeSignature(key),
                ClosureTypeSignature(value)
            ),
            TypeExpr::String => formatter.write_str("String"),
            TypeExpr::Str => formatter.write_str("Str"),
            TypeExpr::Builtin(kind) => formatter.write_str(kind.type_id()),
            TypeExpr::Parameter(parameter) => formatter.write_str(&parameter.name),
        }
    }
}

fn write_signature_types(formatter: &mut fmt::Formatter<'_>, types: &[TypeExpr]) -> fmt::Result {
    types.iter().enumerate().try_for_each(|(index, type_expr)| {
        if index > 0 {
            formatter.write_str("_")?;
        }
        write!(formatter, "{}", ClosureTypeSignature(type_expr))
    })
}

fn write_parameter_signature_types(
    formatter: &mut fmt::Formatter<'_>,
    types: &[TypeExpr],
) -> fmt::Result {
    types.iter().enumerate().try_for_each(|(index, type_expr)| {
        if index > 0 {
            formatter.write_str("_")?;
        }
        write!(formatter, "{}", ClosureTypeSignature(type_expr))
    })
}

fn source_type_signature(source_id: &str) -> String {
    source_id
        .split("::")
        .filter(|segment| !segment.is_empty())
        .map(capitalize)
        .collect()
}

fn primitive_signature(primitive: AstPrimitive) -> String {
    let rust_name = primitive.rust_name();
    rust_name
        .strip_suffix("size")
        .and_then(|prefix| {
            (prefix.len() == 1).then(|| format!("{}Size", prefix.to_ascii_uppercase()))
        })
        .unwrap_or_else(|| capitalize(rust_name))
}

fn capitalize(text: &str) -> String {
    let mut characters = text.chars();
    match characters.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(characters).collect(),
    }
}

fn closure_symbol_case(name: &str) -> String {
    name.chars().enumerate().fold(
        String::with_capacity(name.len() + 4),
        |mut result, (index, character)| {
            if character.is_uppercase() {
                if index > 0 {
                    result.push('_');
                }
                result.push(character.to_ascii_lowercase());
            } else {
                result.push(character);
            }
            result
        },
    )
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{Path, RecordId};

    use super::*;

    fn closure(parameters: Vec<TypeExpr>, returns: ReturnDef) -> FnSig {
        FnSig::new(parameters, returns)
    }

    fn record(id: &str, path: &str) -> TypeExpr {
        TypeExpr::record(RecordId::new(id), Path::single(path))
    }

    #[test]
    fn registration_uses_closure_signature_import_names() {
        let closure = closure(
            vec![TypeExpr::Primitive(AstPrimitive::F64)],
            ReturnDef::Void,
        );
        let registration = incoming_registration(&closure).expect("valid closure registration");

        assert_eq!(
            registration.call().module().as_str(),
            symbol::WASM_CALLBACK_IMPORT_MODULE
        );
        assert_eq!(
            registration.call().name().as_str(),
            "__boltffi_callback_closure____closure__f64_call"
        );
        assert_eq!(
            registration.free().name().as_str(),
            "__boltffi_callback_closure____closure__f64_free"
        );
    }

    #[test]
    fn outgoing_registration_uses_closure_signature_export_names() {
        let closure = closure(
            vec![TypeExpr::Primitive(AstPrimitive::F64)],
            ReturnDef::Void,
        );
        let mut allocator = SymbolAllocator::new();
        let registration =
            outgoing_registration(&mut allocator, &closure).expect("valid closure registration");

        assert_eq!(
            registration.call().name().as_str(),
            "boltffi_closure_0____closure__f64_call"
        );
        assert_eq!(
            registration.free().name().as_str(),
            "boltffi_closure_0____closure__f64_free"
        );
    }

    #[test]
    fn signature_keeps_nested_source_shape() {
        let closure = closure(
            vec![TypeExpr::option(record("demo::Point", "Point"))],
            ReturnDef::value(TypeExpr::result(
                TypeExpr::Primitive(AstPrimitive::I32),
                record("demo::MathError", "MathError"),
            )),
        );

        assert_eq!(
            ClosureSignature::from_closure(&closure).symbol_part(),
            "___closure__opt_demo_point_to_result_i32_err_demo_math_error"
        );
    }

    #[test]
    fn signature_includes_named_type_namespace() {
        let first = closure(vec![record("a::Point", "Point")], ReturnDef::Void);
        let second = closure(vec![record("b::Point", "Point")], ReturnDef::Void);

        assert_eq!(
            ClosureSignature::from_closure(&first).symbol_part(),
            "___closure__a_point"
        );
        assert_eq!(
            ClosureSignature::from_closure(&second).symbol_part(),
            "___closure__b_point"
        );
    }

    #[test]
    fn symbol_case_preserves_callback_signature_casing() {
        assert_eq!(
            closure_symbol_case("__Closure_I32_StringToBool"),
            "___closure__i32__string_to_bool"
        );
    }
}

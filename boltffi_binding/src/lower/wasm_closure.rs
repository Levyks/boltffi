use std::fmt;

use boltffi_ast::{ClosureType, Primitive as AstPrimitive, ReturnDef, TypeExpr};

use crate::{ImportModule, ImportSymbol, SymbolName, wasm32};

use super::{LowerError, symbol};

pub(super) fn registration(
    closure: &ClosureType,
) -> Result<wasm32::ClosureRegistration, LowerError> {
    let module = ImportModule::parse(symbol::WASM_CALLBACK_IMPORT_MODULE.to_owned())?;
    let signature = ClosureSignature::from_closure(closure).symbol_part();
    let call = import_symbol(module.clone(), &signature, "call")?;
    let free = import_symbol(module, &signature, "free")?;
    Ok(wasm32::ClosureRegistration::new(call, free))
}

fn import_symbol(
    module: ImportModule,
    signature: &str,
    action: &str,
) -> Result<ImportSymbol, LowerError> {
    let name = symbol::wasm_callback_import_name("closure", signature, action);
    Ok(ImportSymbol::new(module, SymbolName::parse(name)?))
}

struct ClosureSignature<'a> {
    params: &'a [TypeExpr],
    returns: &'a ReturnDef,
}

impl<'a> ClosureSignature<'a> {
    fn from_closure(closure: &'a ClosureType) -> Self {
        Self {
            params: &closure.parameters,
            returns: &closure.returns,
        }
    }

    fn symbol_part(&self) -> String {
        closure_symbol_case(&format!("__Closure_{self}"))
    }
}

impl fmt::Display for ClosureSignature<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (
            self.params.is_empty(),
            return_signature_is_void(self.returns),
        ) {
            (true, true) => formatter.write_str("Void"),
            (false, true) => write_signature_types(formatter, self.params),
            (true, false) => {
                formatter.write_str("To")?;
                write_return_signature(formatter, self.returns)
            }
            (false, false) => {
                write_signature_types(formatter, self.params)?;
                formatter.write_str("To")?;
                write_return_signature(formatter, self.returns)
            }
        }
    }
}

fn return_signature_is_void(returns: &ReturnDef) -> bool {
    matches!(returns, ReturnDef::Void | ReturnDef::Value(TypeExpr::Unit))
}

fn write_return_signature(formatter: &mut fmt::Formatter<'_>, returns: &ReturnDef) -> fmt::Result {
    match returns {
        ReturnDef::Void => formatter.write_str("Void"),
        ReturnDef::Value(type_expr) => write!(formatter, "{}", ClosureTypeSignature(type_expr)),
    }
}

struct ClosureTypeSignature<'a>(&'a TypeExpr);

impl fmt::Display for ClosureTypeSignature<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            TypeExpr::Primitive(primitive) => formatter.write_str(&primitive_signature(*primitive)),
            TypeExpr::Unit => formatter.write_str("Void"),
            TypeExpr::Record(id) => formatter.write_str(&source_type_name(id.as_str())),
            TypeExpr::Enum(id) => formatter.write_str(&source_type_name(id.as_str())),
            TypeExpr::Class { id, .. } => formatter.write_str(&source_type_name(id.as_str())),
            TypeExpr::Trait { id, .. } => formatter.write_str(&source_type_name(id.as_str())),
            TypeExpr::Closure(_) => formatter.write_str("Closure"),
            TypeExpr::Custom(id) => formatter.write_str(&source_type_name(id.as_str())),
            TypeExpr::SelfType => formatter.write_str("Self"),
            TypeExpr::Vec(inner) => write!(formatter, "Vec{}", ClosureTypeSignature(inner)),
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
            TypeExpr::Map { key, value } => write!(
                formatter,
                "Map{}To{}",
                ClosureTypeSignature(key),
                ClosureTypeSignature(value)
            ),
            TypeExpr::String => formatter.write_str("String"),
            TypeExpr::Bytes => formatter.write_str("Bytes"),
            TypeExpr::Parameter(parameter) => formatter.write_str(&parameter.name),
        }
    }
}

fn write_signature_types(formatter: &mut fmt::Formatter<'_>, types: &[TypeExpr]) -> fmt::Result {
    for (index, type_expr) in types.iter().enumerate() {
        if index > 0 {
            formatter.write_str("_")?;
        }
        write!(formatter, "{}", ClosureTypeSignature(type_expr))?;
    }
    Ok(())
}

fn source_type_name(source_id: &str) -> String {
    source_id
        .rsplit("::")
        .find(|segment| !segment.is_empty())
        .map_or_else(|| source_id.to_owned(), ToOwned::to_owned)
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
    use boltffi_ast::{ClosureKind, RecordId};

    use super::*;

    #[test]
    fn registration_uses_closure_signature_import_names() {
        let closure = ClosureType::new(
            ClosureKind::Fn,
            vec![TypeExpr::Primitive(AstPrimitive::F64)],
            ReturnDef::Void,
        );
        let registration = registration(&closure).expect("valid closure registration");

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
    fn signature_keeps_nested_source_shape() {
        let closure = ClosureType::new(
            ClosureKind::Fn,
            vec![TypeExpr::option(TypeExpr::Record(RecordId::new(
                "demo::Point",
            )))],
            ReturnDef::Value(TypeExpr::result(
                TypeExpr::Primitive(AstPrimitive::I32),
                TypeExpr::Record(RecordId::new("demo::MathError")),
            )),
        );

        assert_eq!(
            ClosureSignature::from_closure(&closure).symbol_part(),
            "___closure__opt_point_to_result_i32_err_math_error"
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

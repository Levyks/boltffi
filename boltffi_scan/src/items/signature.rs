use boltffi_ast::{CanonicalName, ExecutionKind, ParameterDef, ParameterPassing};

use crate::type_expr::Scanner;
use crate::{ScanError, name};

pub(super) fn execution(signature: &syn::Signature) -> ExecutionKind {
    match signature.asyncness {
        Some(_) => ExecutionKind::Async,
        None => ExecutionKind::Sync,
    }
}

pub(super) fn parameter(
    typed: &syn::PatType,
    scanner: &Scanner<'_>,
) -> Result<ParameterDef, ScanError> {
    let binding_name = parameter_name(&typed.pat)?;
    let (source_type, passing) = parameter_type(&typed.ty);
    let mut parameter = ParameterDef::value(binding_name, scanner.scan(source_type)?);
    parameter.passing = passing;
    Ok(parameter)
}

fn parameter_type(ty: &syn::Type) -> (&syn::Type, ParameterPassing) {
    match ty {
        syn::Type::Reference(reference) => {
            let passing = match reference.mutability {
                Some(_) => ParameterPassing::RefMut,
                None => ParameterPassing::Ref,
            };
            (&reference.elem, passing)
        }
        _ => (ty, ParameterPassing::Value),
    }
}

fn parameter_name(pat: &syn::Pat) -> Result<CanonicalName, ScanError> {
    match pat {
        syn::Pat::Ident(binding) => Ok(name::canonical(&binding.ident)),
        _ => Err(ScanError::UnnamedParameter),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModulePath;
    use crate::declared_types::DeclaredTypes;
    use boltffi_ast::{Primitive, TypeExpr};

    fn parameter(source: &str) -> ParameterDef {
        let typed = syn::parse_str::<syn::PatType>(source).expect("parameter");
        let declared_types = DeclaredTypes::new();
        let module = ModulePath::root("demo");
        let scanner = Scanner::new(&declared_types, &module);
        super::parameter(&typed, &scanner).expect("scan")
    }

    #[test]
    fn records_value_parameter_passing() {
        let parameter = parameter("value: i32");

        assert_eq!(parameter.type_expr, TypeExpr::Primitive(Primitive::I32));
        assert_eq!(parameter.passing, ParameterPassing::Value);
    }

    #[test]
    fn records_shared_reference_parameter_passing() {
        let parameter = parameter("value: &i32");

        assert_eq!(parameter.type_expr, TypeExpr::Primitive(Primitive::I32));
        assert_eq!(parameter.passing, ParameterPassing::Ref);
    }

    #[test]
    fn records_mutable_reference_parameter_passing() {
        let parameter = parameter("value: &mut i32");

        assert_eq!(parameter.type_expr, TypeExpr::Primitive(Primitive::I32));
        assert_eq!(parameter.passing, ParameterPassing::RefMut);
    }
}

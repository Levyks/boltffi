use std::collections::HashMap;

use boltffi_binding::{FunctionDecl, Native};

use crate::{
    bridge::jni::{JniBridgeContract, NativeMethod, NativeParameter},
    core::{Error, Result},
    target::kotlin::{
        render::type_name::KotlinType,
        syntax::{ArgumentList, Expression, Identifier, TypeName},
    },
};

const JNI_BRIDGE: &str = "jni";

pub struct NativeMethods<'bridge> {
    methods: HashMap<&'bridge str, &'bridge NativeMethod>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeFunction {
    name: Identifier,
    parameters: Vec<NativeFunctionParameter>,
    returns: TypeName,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeFunctionParameter {
    name: Identifier,
    ty: TypeName,
}

impl<'bridge> NativeMethods<'bridge> {
    pub fn new(bridge: &'bridge JniBridgeContract) -> Self {
        Self {
            methods: bridge
                .methods()
                .iter()
                .map(|method| (method.c_function().name(), method))
                .collect(),
        }
    }

    pub fn function(&self, decl: &FunctionDecl<Native>) -> Result<NativeFunction> {
        self.methods
            .get(decl.symbol().name().as_str())
            .copied()
            .ok_or(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "function declaration has no JNI native method",
            })
            .and_then(NativeFunction::from_method)
    }
}

impl NativeFunction {
    pub fn from_method(method: &NativeMethod) -> Result<Self> {
        Ok(Self {
            name: Identifier::escape(method.c_function().name())?,
            parameters: method
                .parameters()
                .iter()
                .map(NativeFunctionParameter::from_parameter)
                .collect::<Result<Vec<_>>>()?,
            returns: KotlinType::native_return(method.returns())?,
        })
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn parameters(&self) -> &[NativeFunctionParameter] {
        &self.parameters
    }

    pub fn returns(&self) -> &TypeName {
        &self.returns
    }
}

impl NativeFunctionParameter {
    pub fn from_parameter(parameter: &NativeParameter) -> Result<Self> {
        Ok(Self {
            name: Identifier::escape(parameter.name().as_str())?,
            ty: KotlinType::native_parameter(parameter.kind())?,
        })
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn ty(&self) -> &TypeName {
        &self.ty
    }
}

pub struct NativeCall {
    function: Identifier,
    arguments: Vec<Expression>,
}

impl NativeCall {
    pub fn new(function: Identifier, arguments: Vec<Expression>) -> Self {
        Self {
            function,
            arguments,
        }
    }

    pub fn expression(&self) -> Expression {
        Expression::call(
            "Native",
            self.function.clone(),
            self.arguments.iter().cloned().collect::<ArgumentList>(),
        )
    }
}

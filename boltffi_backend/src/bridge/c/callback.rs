use boltffi_binding::{
    CallbackDecl, CallbackId, ExecutionDecl, ImportedMethodDecl, Native, VTableSlot,
};

use crate::core::Result;

use super::{
    Field, Function, Identifier, Parameter, Record, Type, function::Signature, names::Names,
};

/// A native callback vtable declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Callback {
    id: CallbackId,
    vtable: Record,
    register: Function,
    create_handle: Function,
}

impl Callback {
    /// Returns the source callback trait id.
    pub const fn id(&self) -> CallbackId {
        self.id
    }

    /// Returns the callback vtable record.
    pub fn vtable(&self) -> &Record {
        &self.vtable
    }

    /// Returns the callback registration function.
    pub fn register(&self) -> &Function {
        &self.register
    }

    /// Returns the callback handle constructor.
    pub fn create_handle(&self) -> &Function {
        &self.create_handle
    }
}

impl Callback {
    /// Creates the C callback declaration for a lowered callback trait.
    pub fn from_decl(callback: &CallbackDecl<Native>, names: &Names) -> Result<Self> {
        let vtable_name = Identifier::parse(format!("{}VTable", names.callback(callback.id())?))?;
        let vtable = callback.protocol().vtable();
        let free = Field::new(
            vtable.free_slot().as_str(),
            Type::FunctionPointer {
                returns: Box::new(Type::Void),
                params: vec![Type::Uint64],
            },
        )?;
        let clone = Field::new(
            vtable.clone_slot().as_str(),
            Type::FunctionPointer {
                returns: Box::new(Type::Uint64),
                params: vec![Type::Uint64],
            },
        )?;
        let methods = vtable
            .methods()
            .iter()
            .map(|method| Field::callback_method(method, names))
            .collect::<Result<Vec<_>>>()?;
        let vtable = Record::new(
            vtable_name.clone(),
            [free, clone].into_iter().chain(methods).collect(),
        );
        let register = Function::new(
            callback.protocol().register().name().as_str(),
            vec![Parameter::new(
                "vtable",
                Type::ConstPointer(Box::new(Type::Named(vtable_name.clone()))),
            )?],
            Type::Void,
        )?;
        let create_handle = Function::new(
            callback.protocol().create_handle().name().as_str(),
            vec![Parameter::new("handle", Type::Uint64)?],
            Type::CallbackHandle,
        )?;
        Ok(Self {
            id: callback.id(),
            vtable,
            register,
            create_handle,
        })
    }
}

impl Field {
    fn callback_method(
        method: &ImportedMethodDecl<Native, VTableSlot>,
        names: &Names,
    ) -> Result<Self> {
        let signature = Signature::new(names, Vec::new());
        if matches!(
            method.callable().execution(),
            ExecutionDecl::Asynchronous(_)
        ) {
            return Self::async_callback_method(method, &signature);
        }
        let return_params = signature.callback_return_params(method.callable().returns().plan())?;
        let method_params = signature.imported_params(method.callable().params())?;
        let params = std::iter::once(Type::Uint64)
            .chain(
                return_params
                    .into_iter()
                    .map(|parameter| parameter.ty().clone()),
            )
            .chain(
                method_params
                    .into_iter()
                    .map(|parameter| parameter.ty().clone()),
            )
            .collect();
        Self::new(
            method.target().as_str(),
            Type::FunctionPointer {
                returns: Box::new(signature.callback_return_type(
                    method.callable().returns().plan(),
                    method.callable().error(),
                )?),
                params,
            },
        )
    }

    fn async_callback_method(
        method: &ImportedMethodDecl<Native, VTableSlot>,
        signature: &Signature,
    ) -> Result<Self> {
        let method_params = signature.imported_params(method.callable().params())?;
        let completion = signature.async_completion(
            method.callable().returns().plan(),
            method.callable().error(),
        )?;
        let params = std::iter::once(Type::Uint64)
            .chain(
                method_params
                    .into_iter()
                    .map(|parameter| parameter.ty().clone()),
            )
            .chain([completion, Type::MutPointer(Box::new(Type::Void))])
            .collect();
        Self::new(
            method.target().as_str(),
            Type::FunctionPointer {
                returns: Box::new(Type::Void),
                params,
            },
        )
    }
}

use boltffi_binding::{FieldKey, IntrinsicOp, OpRender, TypeRef, ValueRef};

use crate::{
    core::{Error, Result},
    target::python::{codec::value::ValueExpression, cpython::render::primitive},
};

pub struct Operation;

impl Operation {
    fn binary(
        left: Result<String>,
        right: Result<String>,
        operator: &'static str,
    ) -> Result<String> {
        Ok(format!("({} {operator} {})", left?, right?))
    }

    fn single_argument(args: Vec<Result<String>>) -> Result<String> {
        let mut args = args.into_iter().collect::<Result<Vec<_>>>()?;
        match args.len() {
            1 => Ok(args.remove(0)),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "python operation with invalid arity",
            }),
        }
    }
}

impl OpRender for Operation {
    type Expr = Result<String>;

    fn value(&mut self, value: &ValueRef) -> Self::Expr {
        ValueExpression::new(value).render()
    }

    fn byte_count(&mut self, bytes: u64) -> Self::Expr {
        Ok(bytes.to_string())
    }

    fn integer(&mut self, value: i128) -> Self::Expr {
        Ok(value.to_string())
    }

    fn add(&mut self, left: Self::Expr, right: Self::Expr) -> Self::Expr {
        Self::binary(left, right, "+")
    }

    fn mul(&mut self, left: Self::Expr, right: Self::Expr) -> Self::Expr {
        Self::binary(left, right, "*")
    }

    fn eq(&mut self, left: Self::Expr, right: Self::Expr) -> Self::Expr {
        Self::binary(left, right, "==")
    }

    fn field(&mut self, base: Self::Expr, field: &FieldKey) -> Self::Expr {
        ValueExpression::field(base?, field)
    }

    fn intrinsic(&mut self, intrinsic: IntrinsicOp, args: Vec<Self::Expr>) -> Self::Expr {
        let value = Self::single_argument(args)?;
        match intrinsic {
            IntrinsicOp::Utf8ByteCount => Ok(format!("len(str({value}).encode(\"utf-8\"))")),
            IntrinsicOp::SequenceLen => Ok(format!("len({value})")),
            IntrinsicOp::WireSize => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "python wire-size operation",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown python operation",
            }),
        }
    }

    fn size_of(&mut self, ty: &TypeRef) -> Self::Expr {
        match ty {
            TypeRef::Primitive(primitive) => primitive::Runtime::new(*primitive)
                .wire_size()
                .map(|size| size.to_string()),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "python type-size operation",
            }),
        }
    }
}

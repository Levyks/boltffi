//! Source view for one generated `Java_*` native method.
//!
//! The native method contract is shaped for correctness: parameters know their
//! kind, returns know their ABI behavior, and records know their writeback
//! rules. The final C template needs one ordered method body with declarations,
//! borrowed-array setup, direct-record locals, C bridge arguments, status checks,
//! cleanup, and the return expression.
//!
//! This module performs that final projection. It does not reinterpret the C
//! bridge contract or inspect binding IR. It gathers the source-ready views that
//! the Askama method template prints.

use crate::{
    bridge::{
        c::{ArgumentList, Expression, Identifier, TypeFragment},
        jni::{
            NativeMethod, NativeParameter,
            template::method::{
                BorrowedArrayParameterView, NativeParameterView, RecordParameterView,
            },
        },
    },
    core::Result,
};

/// Template input for a generated JNI native method body.
pub struct NativeMethodView {
    pub symbol: Identifier,
    pub c_function: Identifier,
    pub return_type: TypeFragment,
    pub c_result_type: TypeFragment,
    pub parameters: Vec<NativeParameterView>,
    pub borrowed_arrays: Vec<BorrowedArrayParameterView>,
    pub record_arrays: Vec<RecordParameterView>,
    pub arguments: ArgumentList,
    pub returns_void: bool,
    pub returns_boolean: bool,
    pub returns_bytes: bool,
    pub returns_record: bool,
    pub returns_callback: bool,
    pub return_value: Expression,
    pub checks_status: bool,
    pub uses_continuations: bool,
}

impl NativeMethodView {
    pub fn from_method(method: &NativeMethod) -> Result<Self> {
        Ok(Self {
            symbol: method.symbol().as_identifier().clone(),
            c_function: Identifier::parse(method.c_function().name())?,
            return_type: method.returns().jni_type(),
            c_result_type: method.returns().c_result_type()?,
            parameters: method
                .parameters()
                .iter()
                .map(NativeParameterView::from_parameter)
                .collect(),
            borrowed_arrays: method
                .parameters()
                .iter()
                .flat_map(Self::borrowed_array)
                .collect::<Result<Vec<_>>>()?,
            record_arrays: method
                .parameters()
                .iter()
                .filter_map(|parameter| parameter.record().map(RecordParameterView::from_record))
                .collect(),
            arguments: ArgumentList::from_iter(
                method
                    .parameters()
                    .iter()
                    .map(NativeParameter::c_arguments)
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
                    .flatten(),
            ),
            returns_void: method.returns_void(),
            returns_boolean: method.returns_boolean(),
            returns_bytes: method.returns_bytes(),
            returns_record: method.returns_record(),
            returns_callback: method.returns_callback(),
            return_value: method
                .returns()
                .return_expression(Expression::identifier(Identifier::parse("result")?))?,
            checks_status: method.checks_status(),
            uses_continuations: method
                .parameters()
                .iter()
                .any(NativeParameter::is_continuation),
        })
    }

    fn borrowed_array(parameter: &NativeParameter) -> Option<Result<BorrowedArrayParameterView>> {
        parameter
            .bytes()
            .map(|parameter| Ok(BorrowedArrayParameterView::from_bytes(parameter)))
            .or_else(|| {
                parameter
                    .direct_vector()
                    .map(BorrowedArrayParameterView::from_direct_vector)
            })
    }
}

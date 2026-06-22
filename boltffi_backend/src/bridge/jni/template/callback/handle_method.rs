//! Source fields for calling Rust-owned callback handles from the JVM.
//!
//! Returned callback handles live in native storage and expose their methods
//! through a C vtable. The generated JNI method takes the handle token from
//! Java, validates the stored vtable, prepares the Java-provided arguments, and
//! invokes the matching vtable slot.
//!
//! This module prepares the source view for that method body. It borrows the
//! same parameter, array, record, and return views used by normal native methods,
//! so a returned callback handle does not get a second conversion model just
//! because the call starts from a `jlong` token.

use crate::{
    bridge::{
        c::{ArgumentList, Expression, Identifier, TypeFragment},
        jni::{
            CallbackHandleMethod,
            template::method::{
                BorrowedArrayParameterView, NativeParameterView, RecordParameterView,
            },
        },
    },
    core::Result,
};

pub struct CallbackHandleMethodView {
    pub symbol: Identifier,
    pub return_type: TypeFragment,
    pub c_result_type: TypeFragment,
    pub vtable_type: Identifier,
    pub slot: Identifier,
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
}

impl CallbackHandleMethodView {
    pub fn from_method(method: &CallbackHandleMethod) -> Result<Self> {
        Ok(Self {
            symbol: method.symbol().as_identifier().clone(),
            return_type: method.jni_type(),
            c_result_type: method.c_result_type()?,
            vtable_type: method.vtable_type().clone(),
            slot: method.slot().clone(),
            parameters: method
                .parameters()
                .iter()
                .map(NativeParameterView::from_parameter)
                .collect(),
            borrowed_arrays: method
                .parameters()
                .iter()
                .flat_map(BorrowedArrayParameterView::from_parameter)
                .collect::<Result<Vec<_>>>()?,
            record_arrays: method
                .parameters()
                .iter()
                .filter_map(|parameter| parameter.record().map(RecordParameterView::from_record))
                .collect(),
            arguments: method.arguments()?,
            returns_void: method.returns_void(),
            returns_boolean: method.returns_boolean(),
            returns_bytes: method.returns_bytes(),
            returns_record: method.returns_record(),
            returns_callback: method.returns_callback(),
            return_value: method
                .return_value(Expression::identifier(Identifier::parse("result")?))?,
            checks_status: method.checks_status(),
        })
    }
}

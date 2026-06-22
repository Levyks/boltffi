use crate::bridge::{
    c::{Expression, Identifier, Literal, Statement, TypeFragment},
    jni::{ClosureArgument, ClosureRegistration},
};

pub struct ClosureRegistrationView {
    pub class: Literal,
    pub global_class: Identifier,
    pub call_method: Identifier,
    pub free_method: Identifier,
    pub load: Identifier,
    pub unload: Identifier,
    pub call: Identifier,
    pub release: Identifier,
    pub c_return_type: TypeFragment,
    pub returns_void: bool,
    pub returns_byte_array: bool,
    pub returns_bytes: bool,
    pub returns_record: bool,
    pub method_signature: Literal,
    pub call_method_suffix: String,
    pub failure_value: Expression,
    pub arguments: Vec<ClosureArgumentView>,
}

pub struct CallbackClosureHandleView {
    pub ty: Identifier,
    pub new: Identifier,
    pub ref_: Identifier,
    pub release: Identifier,
    pub call_symbol: Identifier,
    pub release_symbol: Identifier,
    pub call_field: Statement,
    pub jni_return_type: TypeFragment,
    pub failure_value: Expression,
    pub closure: ClosureRegistrationView,
}

pub struct ClosureArgumentView {
    pub name: Identifier,
    pub c_type: TypeFragment,
    pub jni_type: TypeFragment,
}

impl ClosureRegistrationView {
    pub fn from_registration(registration: &ClosureRegistration) -> Self {
        Self {
            class: Literal::string(&registration.class().as_jni_class_name()),
            global_class: registration.global_class().clone(),
            call_method: registration.call_method().clone(),
            free_method: registration.free_method().clone(),
            load: registration.load().clone(),
            unload: registration.unload().clone(),
            call: registration.call().clone(),
            release: registration.release().clone(),
            c_return_type: registration.c_return_type().clone(),
            returns_void: registration.returns_void(),
            returns_byte_array: registration.returns_byte_array(),
            returns_bytes: registration.returns_bytes(),
            returns_record: registration.returns_record(),
            method_signature: Literal::string(&registration.method_signature()),
            call_method_suffix: registration
                .call_method_suffix()
                .unwrap_or_default()
                .to_owned(),
            failure_value: registration
                .failure_value()
                .unwrap_or_else(|| Expression::literal(Literal::integer_zero())),
            arguments: registration
                .arguments()
                .iter()
                .map(ClosureArgumentView::from_argument)
                .collect(),
        }
    }
}

impl CallbackClosureHandleView {
    pub fn from_registration(registration: &ClosureRegistration) -> Option<Self> {
        registration.callback_handle().map(|handle| Self {
            ty: handle.ty().clone(),
            new: handle.new_function().clone(),
            ref_: handle.ref_function().clone(),
            release: handle.release_function().clone(),
            call_symbol: handle.call_symbol().as_identifier().clone(),
            release_symbol: handle.release_symbol().as_identifier().clone(),
            call_field: handle.call_field().clone(),
            jni_return_type: registration.callback_return_type(),
            failure_value: registration
                .callback_failure_value()
                .unwrap_or_else(|| Expression::literal(Literal::integer_zero())),
            closure: ClosureRegistrationView::from_registration(registration),
        })
    }
}

impl ClosureArgumentView {
    fn from_argument(argument: &ClosureArgument) -> Self {
        Self {
            name: argument.name().clone(),
            c_type: argument.c_type().clone(),
            jni_type: argument.jni_type(),
        }
    }
}

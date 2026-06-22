use crate::bridge::{
    c::{ArgumentList, Expression, Identifier, Literal, Statement, TypeFragment},
    jni::{
        CallbackBytesArgument, CallbackCParameter, CallbackClosureArgument,
        CallbackCompletionArgument, CallbackHandleArgument, CallbackMethod, CallbackRecordArgument,
        CallbackRegistration,
    },
};

pub struct CallbackRegistrationView {
    pub class: Literal,
    pub global_class: Identifier,
    pub free_method: Identifier,
    pub clone_method: Identifier,
    pub load: Identifier,
    pub unload: Identifier,
    pub vtable_type: Identifier,
    pub vtable: Identifier,
    pub register: Identifier,
    pub free: Identifier,
    pub clone: Identifier,
    pub methods: Vec<CallbackMethodView>,
}

pub struct CallbackMethodView {
    pub function: Identifier,
    pub method: Identifier,
    pub method_id: Identifier,
    pub signature: Literal,
    pub c_return_type: TypeFragment,
    pub returns_void: bool,
    pub returns_byte_array: bool,
    pub returns_bytes: bool,
    pub returns_record: bool,
    pub call_method_suffix: String,
    pub failure_value: Expression,
    pub c_parameters: Vec<CallbackCParameterView>,
    pub byte_arrays: Vec<CallbackBytesArgumentView>,
    pub record_arrays: Vec<CallbackRecordArgumentView>,
    pub callback_handles: Vec<CallbackHandleArgumentView>,
    pub closure_handles: Vec<CallbackClosureArgumentView>,
    pub completions: Vec<CallbackCompletionArgumentView>,
    pub jni_arguments: ArgumentList,
}

pub struct CallbackCParameterView {
    pub declaration: Statement,
}

pub struct CallbackBytesArgumentView {
    pub name: Identifier,
    pub pointer: Identifier,
    pub length: Identifier,
}

pub struct CallbackRecordArgumentView {
    pub array: Identifier,
    pub parameter: Identifier,
}

pub struct CallbackHandleArgumentView {
    pub handle: Identifier,
    pub parameter: Identifier,
}

pub struct CallbackClosureArgumentView {
    pub handle: Identifier,
    pub call: Identifier,
    pub context: Identifier,
    pub release: Identifier,
    pub handle_new: Identifier,
    pub handle_release: Identifier,
}

pub struct CallbackCompletionArgumentView {
    pub callback: Identifier,
    pub failure_arguments: ArgumentList,
}

impl CallbackRegistrationView {
    pub fn from_registration(registration: &CallbackRegistration) -> Self {
        Self {
            class: Literal::string(&registration.class().as_jni_class_name()),
            global_class: registration.global_class().clone(),
            free_method: registration.free_method().clone(),
            clone_method: registration.clone_method().clone(),
            load: registration.load().clone(),
            unload: registration.unload().clone(),
            vtable_type: registration.vtable_type().clone(),
            vtable: registration.vtable().clone(),
            register: registration.register().clone(),
            free: registration.free().clone(),
            clone: registration.clone_callback().clone(),
            methods: registration
                .methods()
                .iter()
                .map(CallbackMethodView::from_method)
                .collect(),
        }
    }
}

impl CallbackMethodView {
    pub fn from_method(method: &CallbackMethod) -> Self {
        Self {
            function: method.function().clone(),
            method: method.method().clone(),
            method_id: method.method_id().clone(),
            signature: Literal::string(method.signature()),
            c_return_type: method.c_return_type().clone(),
            returns_void: method.returns_void(),
            returns_byte_array: method.returns_byte_array(),
            returns_bytes: method.returns_bytes(),
            returns_record: method.returns_record(),
            call_method_suffix: method.call_method_suffix().unwrap_or_default().to_owned(),
            failure_value: method
                .failure_value()
                .unwrap_or_else(|| Expression::literal(Literal::integer_zero())),
            c_parameters: method
                .c_parameters()
                .iter()
                .map(CallbackCParameterView::from_parameter)
                .collect(),
            byte_arrays: method
                .byte_arrays()
                .iter()
                .map(CallbackBytesArgumentView::from_argument)
                .collect(),
            record_arrays: method
                .record_arrays()
                .iter()
                .map(CallbackRecordArgumentView::from_argument)
                .collect(),
            callback_handles: method
                .callback_handles()
                .iter()
                .map(CallbackHandleArgumentView::from_argument)
                .collect(),
            closure_handles: method
                .closure_handles()
                .iter()
                .map(CallbackClosureArgumentView::from_argument)
                .collect(),
            completions: method
                .completions()
                .iter()
                .map(CallbackCompletionArgumentView::from_argument)
                .collect(),
            jni_arguments: method.jni_arguments(),
        }
    }
}

impl CallbackCParameterView {
    pub fn from_parameter(parameter: &CallbackCParameter) -> Self {
        Self {
            declaration: parameter.declaration().clone(),
        }
    }
}

impl CallbackBytesArgumentView {
    pub fn from_argument(argument: &CallbackBytesArgument<'_>) -> Self {
        Self {
            name: argument.name().clone(),
            pointer: argument.pointer().clone(),
            length: argument.length().clone(),
        }
    }
}

impl CallbackRecordArgumentView {
    pub fn from_argument(argument: &CallbackRecordArgument<'_>) -> Self {
        Self {
            array: argument.array().clone(),
            parameter: argument.parameter().clone(),
        }
    }
}

impl CallbackHandleArgumentView {
    pub fn from_argument(argument: &CallbackHandleArgument<'_>) -> Self {
        Self {
            handle: argument.handle().clone(),
            parameter: argument.parameter().clone(),
        }
    }
}

impl CallbackClosureArgumentView {
    pub fn from_argument(argument: &CallbackClosureArgument<'_>) -> Self {
        Self {
            handle: argument.handle().clone(),
            call: argument.call().clone(),
            context: argument.context().clone(),
            release: argument.release().clone(),
            handle_new: argument.handle_new().clone(),
            handle_release: argument.handle_release().clone(),
        }
    }
}

impl CallbackCompletionArgumentView {
    pub fn from_argument(argument: &CallbackCompletionArgument<'_>) -> Self {
        Self {
            callback: argument.callback().clone(),
            failure_arguments: argument.failure_arguments().clone(),
        }
    }
}

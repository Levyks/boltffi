use std::collections::HashSet;

use boltffi_ffi_rules::naming::{LibraryName, Name};

use crate::ir::types::BuiltinKind;

use super::JavaVersion;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaAsyncMode {
    CompletableFuture,
    VirtualThread,
}

impl JavaAsyncMode {
    pub fn from_version(version: JavaVersion) -> Self {
        if version.supports_virtual_threads() {
            Self::VirtualThread
        } else {
            Self::CompletableFuture
        }
    }

    pub fn is_completable_future(&self) -> bool {
        matches!(self, Self::CompletableFuture)
    }

    pub fn is_virtual_thread(&self) -> bool {
        matches!(self, Self::VirtualThread)
    }
}

#[derive(Debug, Clone)]
pub struct JavaAsyncCall {
    pub poll: String,
    pub complete: String,
    pub cancel: String,
    pub free: String,
    pub complete_return_plan: JavaReturnPlan,
}

#[derive(Debug, Clone)]
pub struct JavaModule {
    pub package_name: String,
    pub class_name: String,
    pub lib_name: Name<LibraryName>,
    pub desktop_loader: bool,
    pub java_version: JavaVersion,
    pub async_mode: JavaAsyncMode,
    pub builtins: JavaBuiltinSet,
    pub prefix: String,
    pub records: Vec<JavaRecord>,
    pub enums: Vec<JavaEnum>,
    pub closures: Vec<JavaClosureInterface>,
    pub callbacks: Vec<JavaCallbackTrait>,
    pub async_callback_invokers: Vec<JavaAsyncCallbackInvoker>,
    pub functions: Vec<JavaFunction>,
    pub classes: Vec<JavaClass>,
}

impl JavaModule {
    pub fn package_path(&self) -> String {
        self.package_name.replace('.', "/")
    }

    pub fn has_async(&self) -> bool {
        self.functions.iter().any(|f| f.async_call.is_some())
            || self
                .classes
                .iter()
                .any(|c| c.methods.iter().any(|m| m.async_call.is_some()))
    }

    pub fn has_async_callbacks(&self) -> bool {
        self.callbacks
            .iter()
            .any(JavaCallbackTrait::has_async_methods)
    }

    pub fn uses_completable_future(&self) -> bool {
        self.has_async() || self.has_async_callbacks() || self.has_streams()
    }

    pub fn has_streams(&self) -> bool {
        self.classes.iter().any(|c| c.has_streams())
    }

    pub fn has_wire_params(&self) -> bool {
        self.functions
            .iter()
            .any(|function| function.input_bindings.has_wire_writers())
            || self.classes.iter().any(|c| c.has_wire_params())
            || self.records.iter().any(JavaRecord::has_wire_params)
            || self.enums.iter().any(JavaEnum::has_wire_params)
    }

    pub fn needs_wire_writer(&self) -> bool {
        self.has_wire_params()
            || !self.records.is_empty()
            || self.has_data_enums()
            || self.uses_callback_wire_writer()
    }

    pub fn has_data_enums(&self) -> bool {
        self.enums
            .iter()
            .any(|enumeration| enumeration.is_sealed() || enumeration.is_abstract())
    }

    pub fn has_closures(&self) -> bool {
        !self.closures.is_empty()
    }

    pub fn has_callbacks(&self) -> bool {
        !self.callbacks.is_empty()
    }

    pub fn uses_duration_builtin(&self) -> bool {
        self.builtins.uses_duration()
    }

    pub fn uses_system_time_builtin(&self) -> bool {
        self.builtins.uses_system_time()
    }

    pub fn uses_uuid_builtin(&self) -> bool {
        self.builtins.uses_uuid()
    }

    pub fn uses_url_builtin(&self) -> bool {
        self.builtins.uses_url()
    }

    fn uses_callback_wire_writer(&self) -> bool {
        self.closures
            .iter()
            .any(JavaClosureInterface::requires_wire_writer)
            || self
                .callbacks
                .iter()
                .any(JavaCallbackTrait::requires_wire_writer)
    }
}

#[derive(Debug, Clone, Default)]
pub struct JavaBuiltinSet {
    kinds: HashSet<BuiltinKind>,
}

impl JavaBuiltinSet {
    pub fn from_kinds(kinds: HashSet<BuiltinKind>) -> Self {
        Self { kinds }
    }

    fn uses_duration(&self) -> bool {
        self.kinds.contains(&BuiltinKind::Duration)
    }

    fn uses_system_time(&self) -> bool {
        self.kinds.contains(&BuiltinKind::SystemTime)
    }

    fn uses_uuid(&self) -> bool {
        self.kinds.contains(&BuiltinKind::Uuid)
    }

    fn uses_url(&self) -> bool {
        self.kinds.contains(&BuiltinKind::Url)
    }
}

#[derive(Debug, Clone)]
pub struct JavaEnum {
    pub doc: Option<String>,
    pub class_name: String,
    pub kind: JavaEnumKind,
    pub value_type: String,
    pub variants: Vec<JavaEnumVariant>,
    pub constructors: Vec<JavaValueTypeConstructor>,
    pub methods: Vec<JavaValueTypeMethod>,
}

impl JavaEnum {
    pub fn tag_literal(&self, tag: &i128) -> String {
        match self.value_type.as_str() {
            "byte" => format!("(byte) {}", tag),
            "short" => format!("(short) {}", tag),
            "long" => format!("{}L", tag),
            _ => tag.to_string(),
        }
    }

    pub fn is_c_style(&self) -> bool {
        matches!(self.kind, JavaEnumKind::CStyle)
    }

    pub fn is_error(&self) -> bool {
        matches!(
            self.kind,
            JavaEnumKind::Error | JavaEnumKind::ErrorAbstractClass
        )
    }

    pub fn is_sealed(&self) -> bool {
        matches!(self.kind, JavaEnumKind::SealedInterface)
    }

    pub fn is_abstract(&self) -> bool {
        matches!(
            self.kind,
            JavaEnumKind::AbstractClass | JavaEnumKind::ErrorAbstractClass
        )
    }

    pub fn has_constructors(&self) -> bool {
        !self.constructors.is_empty()
    }

    pub fn has_static_methods(&self) -> bool {
        self.methods.iter().any(|method| method.is_static)
    }

    pub fn has_instance_methods(&self) -> bool {
        self.methods.iter().any(|method| !method.is_static)
    }

    pub fn has_wire_params(&self) -> bool {
        self.constructors
            .iter()
            .any(|constructor| constructor.input_bindings.has_wire_writers())
            || self
                .methods
                .iter()
                .any(|method| method.input_bindings.has_wire_writers())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaEnumKind {
    CStyle,
    Error,
    SealedInterface,
    AbstractClass,
    ErrorAbstractClass,
}

#[derive(Debug, Clone)]
pub struct JavaEnumVariant {
    pub doc: Option<String>,
    pub name: String,
    pub tag: i128,
    pub fields: Vec<JavaEnumField>,
}

impl JavaEnumVariant {
    pub fn is_unit(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn message_field_name(&self) -> Option<&str> {
        self.fields
            .iter()
            .find(|field| field.name == "message" && field.java_type == "String")
            .map(|field| field.name.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct JavaEnumField {
    pub doc: Option<String>,
    pub name: String,
    pub java_type: String,
    pub wire_decode_expr: String,
    pub wire_size_expr: String,
    pub wire_encode_expr: String,
    pub equals_expr: String,
    pub hash_expr: String,
}

#[derive(Debug, Clone)]
pub struct JavaRecord {
    pub doc: Option<String>,
    pub shape: JavaRecordShape,
    pub is_error: bool,
    pub class_name: String,
    pub fields: Vec<JavaRecordField>,
    pub default_constructors: Vec<JavaRecordDefaultConstructor>,
    pub blittable_layout: Option<JavaBlittableLayout>,
    pub constructors: Vec<JavaValueTypeConstructor>,
    pub methods: Vec<JavaValueTypeMethod>,
}

impl JavaRecord {
    pub fn message_field(&self) -> Option<&JavaRecordField> {
        self.fields
            .iter()
            .find(|field| field.name == "message" && field.java_type == "String")
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn is_blittable(&self) -> bool {
        self.blittable_layout.is_some()
    }

    pub fn uses_native_record_syntax(&self) -> bool {
        matches!(self.shape, JavaRecordShape::NativeRecord) && !self.is_error
    }

    pub fn has_constructors(&self) -> bool {
        !self.constructors.is_empty()
    }

    pub fn has_default_constructors(&self) -> bool {
        !self.default_constructors.is_empty()
    }

    pub fn has_static_methods(&self) -> bool {
        self.methods.iter().any(|method| method.is_static)
    }

    pub fn has_instance_methods(&self) -> bool {
        self.methods.iter().any(|method| !method.is_static)
    }

    pub fn has_wire_params(&self) -> bool {
        self.constructors
            .iter()
            .any(|constructor| constructor.input_bindings.has_wire_writers())
            || self
                .methods
                .iter()
                .any(|method| method.input_bindings.has_wire_writers())
    }
}

#[derive(Debug, Clone)]
pub struct JavaBlittableLayout {
    pub struct_size: usize,
    pub fields: Vec<JavaBlittableField>,
}

#[derive(Debug, Clone)]
pub struct JavaBlittableField {
    pub name: String,
    pub const_name: String,
    pub offset: usize,
    pub decode_expr: String,
    pub encode_expr: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaRecordShape {
    ClassicClass,
    NativeRecord,
}

#[derive(Debug, Clone)]
pub struct JavaRecordField {
    pub doc: Option<String>,
    pub name: String,
    pub java_type: String,
    pub default_value: Option<String>,
    pub wire_decode_expr: String,
    pub wire_size_expr: String,
    pub wire_encode_expr: String,
    pub equals_expr: String,
    pub hash_expr: String,
}

#[derive(Debug, Clone)]
pub struct JavaRecordDefaultConstructor {
    pub params: Vec<JavaRecordDefaultConstructorParam>,
    pub arguments: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct JavaRecordDefaultConstructorParam {
    pub name: String,
    pub java_type: String,
}

#[derive(Debug, Clone)]
pub struct JavaReturnPlan {
    pub native_return_type: String,
    pub render: JavaReturnRender,
}

#[derive(Debug, Clone)]
pub enum JavaReturnRender {
    Void,
    Direct,
    CStyleEnum {
        class_name: String,
    },
    Decode {
        decode_expr: String,
    },
    Handle {
        class_name: String,
        nullable: bool,
    },
    Callback {
        callbacks_class_name: String,
        nullable: bool,
    },
    Result {
        ok_decode_expr: String,
        err_decode_expr: String,
        err_is_string: bool,
        err_exception_class: Option<String>,
        err_throw_direct: bool,
    },
}

impl JavaReturnPlan {
    pub fn is_void(&self) -> bool {
        matches!(self.render, JavaReturnRender::Void)
    }

    pub fn is_direct(&self) -> bool {
        matches!(self.render, JavaReturnRender::Direct)
    }

    pub fn is_c_style_enum(&self) -> bool {
        matches!(self.render, JavaReturnRender::CStyleEnum { .. })
    }

    pub fn is_decode(&self) -> bool {
        matches!(self.render, JavaReturnRender::Decode { .. })
    }

    pub fn is_handle(&self) -> bool {
        matches!(self.render, JavaReturnRender::Handle { .. })
    }

    pub fn is_callback(&self) -> bool {
        matches!(self.render, JavaReturnRender::Callback { .. })
    }

    pub fn is_result(&self) -> bool {
        matches!(self.render, JavaReturnRender::Result { .. })
    }

    pub fn result_ok_decode(&self) -> &str {
        match &self.render {
            JavaReturnRender::Result { ok_decode_expr, .. } => ok_decode_expr,
            _ => "",
        }
    }

    pub fn result_err_decode(&self) -> &str {
        match &self.render {
            JavaReturnRender::Result {
                err_decode_expr, ..
            } => err_decode_expr,
            _ => "",
        }
    }

    pub fn result_err_is_string(&self) -> bool {
        matches!(
            self.render,
            JavaReturnRender::Result {
                err_is_string: true,
                ..
            }
        )
    }

    pub fn result_err_exception_class(&self) -> &str {
        match &self.render {
            JavaReturnRender::Result {
                err_exception_class: Some(class),
                ..
            } => class,
            _ => "",
        }
    }

    pub fn result_has_typed_exception(&self) -> bool {
        matches!(
            self.render,
            JavaReturnRender::Result {
                err_exception_class: Some(_),
                ..
            }
        ) || matches!(
            self.render,
            JavaReturnRender::Result {
                err_throw_direct: true,
                ..
            }
        )
    }

    pub fn result_err_throws_directly(&self) -> bool {
        matches!(
            self.render,
            JavaReturnRender::Result {
                err_throw_direct: true,
                ..
            }
        )
    }

    pub fn decode_expr(&self) -> &str {
        match &self.render {
            JavaReturnRender::Decode { decode_expr } => decode_expr,
            _ => "",
        }
    }

    pub fn c_style_enum_class(&self) -> &str {
        match &self.render {
            JavaReturnRender::CStyleEnum { class_name } => class_name,
            _ => "",
        }
    }

    pub fn handle_class(&self) -> &str {
        match &self.render {
            JavaReturnRender::Handle { class_name, .. } => class_name,
            _ => "",
        }
    }

    pub fn handle_nullable(&self) -> bool {
        matches!(self.render, JavaReturnRender::Handle { nullable: true, .. })
    }

    pub fn callback_bridge_class(&self) -> &str {
        match &self.render {
            JavaReturnRender::Callback {
                callbacks_class_name,
                ..
            } => callbacks_class_name,
            _ => "",
        }
    }

    pub fn callback_nullable(&self) -> bool {
        matches!(
            self.render,
            JavaReturnRender::Callback { nullable: true, .. }
        )
    }
}

#[derive(Debug, Clone)]
pub struct JavaFunction {
    pub doc: Option<String>,
    pub name: String,
    pub ffi_name: String,
    pub params: Vec<JavaParam>,
    pub return_type: String,
    pub return_plan: JavaReturnPlan,
    pub input_bindings: JavaInputBindings,
    pub async_call: Option<JavaAsyncCall>,
}

impl JavaFunction {
    pub fn is_async(&self) -> bool {
        self.async_call.is_some()
    }

    pub fn boxed_return_type(&self) -> &str {
        box_java_type(&self.return_type)
    }

    pub fn native_return_type(&self) -> &str {
        &self.return_plan.native_return_type
    }
}

#[derive(Debug, Clone)]
pub struct JavaWireWriter {
    pub binding_name: String,
    pub param_name: String,
    pub size_expr: String,
    pub encode_expr: String,
}

#[derive(Debug, Clone)]
pub struct JavaDirectCompositeInput {
    pub binding_name: String,
    pub param_name: String,
    pub declaration_expr: String,
}

impl JavaDirectCompositeInput {
    pub fn declaration(&self) -> String {
        format!(
            "java.nio.ByteBuffer {} = {}",
            self.binding_name, self.declaration_expr
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct JavaInputBindings {
    pub direct_composites: Vec<JavaDirectCompositeInput>,
    pub wire_writers: Vec<JavaWireWriter>,
}

impl JavaInputBindings {
    pub fn is_empty(&self) -> bool {
        self.direct_composites.is_empty() && self.wire_writers.is_empty()
    }

    pub fn has_wire_writers(&self) -> bool {
        !self.wire_writers.is_empty()
    }

    pub fn binding_name_for(&self, param_name: &str) -> Option<&str> {
        self.direct_composites
            .iter()
            .find(|binding| binding.param_name == param_name)
            .map(|binding| binding.binding_name.as_str())
            .or_else(|| {
                self.wire_writers
                    .iter()
                    .find(|binding| binding.param_name == param_name)
                    .map(|binding| binding.binding_name.as_str())
            })
    }
}

#[derive(Debug, Clone)]
pub struct JavaParam {
    pub name: String,
    pub java_type: String,
    pub native_type: String,
    pub native_expr: String,
}

#[derive(Debug, Clone)]
pub struct JavaNativeParam {
    pub name: String,
    pub native_type: String,
    pub expr: String,
}

#[derive(Debug, Clone)]
pub struct JavaValueTypeConstructor {
    pub doc: Option<String>,
    pub name: String,
    pub params: Vec<JavaParam>,
    pub native_params: Vec<JavaNativeParam>,
    pub return_type: String,
    pub return_plan: JavaReturnPlan,
    pub input_bindings: JavaInputBindings,
    pub ffi_name: String,
}

#[derive(Debug, Clone)]
pub struct JavaValueTypeMethod {
    pub doc: Option<String>,
    pub name: String,
    pub ffi_name: String,
    pub is_static: bool,
    pub params: Vec<JavaParam>,
    pub native_params: Vec<JavaNativeParam>,
    pub return_type: String,
    pub return_plan: JavaReturnPlan,
    pub input_bindings: JavaInputBindings,
}

#[derive(Debug, Clone)]
pub struct JavaStream {
    pub doc: Option<String>,
    pub name: String,
    pub item_type: String,
    pub pop_batch_items_expr: String,
    pub subscribe: String,
    pub poll: String,
    pub pop_batch: String,
    pub wait: String,
    pub unsubscribe: String,
    pub free: String,
    pub mode: JavaStreamMode,
}

#[derive(Debug, Clone)]
pub enum JavaStreamMode {
    Async,
    Batch,
    Callback,
}

#[derive(Debug, Clone)]
pub struct JavaClass {
    pub doc: Option<String>,
    pub class_name: String,
    pub ffi_free: String,
    pub constructors: Vec<JavaConstructor>,
    pub methods: Vec<JavaClassMethod>,
    pub streams: Vec<JavaStream>,
}

impl JavaClass {
    pub fn has_factory_constructors(&self) -> bool {
        self.constructors
            .iter()
            .any(|c| matches!(c.kind, JavaConstructorKind::Factory))
    }

    pub fn has_static_methods(&self) -> bool {
        self.methods.iter().any(|m| m.is_static)
    }

    pub fn has_async_methods(&self) -> bool {
        self.methods.iter().any(|m| m.async_call.is_some())
    }

    pub fn has_streams(&self) -> bool {
        !self.streams.is_empty()
    }

    pub fn has_wire_params(&self) -> bool {
        self.constructors
            .iter()
            .any(|constructor| constructor.input_bindings.has_wire_writers())
            || self
                .methods
                .iter()
                .any(|method| method.input_bindings.has_wire_writers())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaConstructorKind {
    Primary,
    Factory,
    Secondary,
}

#[derive(Debug, Clone)]
pub struct JavaConstructor {
    pub doc: Option<String>,
    pub kind: JavaConstructorKind,
    pub name: String,
    pub is_fallible: bool,
    pub params: Vec<JavaParam>,
    pub ffi_name: String,
    pub input_bindings: JavaInputBindings,
}

impl JavaConstructor {
    pub fn is_factory(&self) -> bool {
        matches!(self.kind, JavaConstructorKind::Factory)
    }
}

#[derive(Debug, Clone)]
pub struct JavaClassMethod {
    pub doc: Option<String>,
    pub name: String,
    pub ffi_name: String,
    pub is_static: bool,
    pub params: Vec<JavaParam>,
    pub return_type: String,
    pub return_plan: JavaReturnPlan,
    pub input_bindings: JavaInputBindings,
    pub async_call: Option<JavaAsyncCall>,
}

impl JavaClassMethod {
    pub fn is_async(&self) -> bool {
        self.async_call.is_some()
    }

    pub fn boxed_return_type(&self) -> &str {
        box_java_type(&self.return_type)
    }

    pub fn native_return_type(&self) -> &str {
        &self.return_plan.native_return_type
    }
}

#[derive(Debug, Clone)]
pub struct JavaClosureInterface {
    pub doc: Option<String>,
    pub invoke_doc: Option<String>,
    pub interface_name: String,
    pub callback_id: String,
    pub callbacks_class_name: String,
    pub supports_proxy_wrap: bool,
    pub supports_cleaner: bool,
    pub params: Vec<JavaBridgeParam>,
    pub return_info: Option<JavaBridgeReturn>,
    pub proxy: JavaCallbackProxySyncMethod,
}

impl JavaClosureInterface {
    pub fn is_void_return(&self) -> bool {
        self.return_info.is_none()
    }

    pub fn boxed_return_type(&self) -> &str {
        self.return_info
            .as_ref()
            .map(JavaBridgeReturn::java_type)
            .map(box_java_type)
            .unwrap_or("Void")
    }

    pub fn requires_wire_writer(&self) -> bool {
        self.return_info
            .as_ref()
            .is_some_and(JavaBridgeReturn::requires_wire_writer)
    }

    pub fn proxy_clone_native_name(&self) -> String {
        format!("boltffi{}Clone", self.callbacks_class_name)
    }

    pub fn proxy_release_native_name(&self) -> String {
        format!("boltffi{}Release", self.callbacks_class_name)
    }
}

#[derive(Debug, Clone)]
pub struct JavaCallbackTrait {
    pub doc: Option<String>,
    pub interface_name: String,
    pub callback_id: String,
    pub supports_cleaner: bool,
    pub sync_methods: Vec<JavaSyncCallbackMethod>,
    pub async_methods: Vec<JavaAsyncCallbackMethod>,
}

impl JavaCallbackTrait {
    pub fn callbacks_class_name(&self) -> String {
        format!("{}Callbacks", self.interface_name)
    }

    pub fn proxy_clone_native_name(&self) -> String {
        format!("boltffiCallback{}Clone", self.interface_name)
    }

    pub fn proxy_release_native_name(&self) -> String {
        format!("boltffiCallback{}Release", self.interface_name)
    }

    pub fn has_async_methods(&self) -> bool {
        !self.async_methods.is_empty()
    }

    pub fn requires_wire_writer(&self) -> bool {
        self.sync_methods
            .iter()
            .any(JavaSyncCallbackMethod::requires_wire_writer)
            || self
                .async_methods
                .iter()
                .any(JavaAsyncCallbackMethod::requires_wire_writer)
    }
}

#[derive(Debug, Clone)]
pub struct JavaBridgeParam {
    pub name: String,
    pub java_type: String,
    pub jni_type: String,
    pub decode_expr: String,
}

#[derive(Debug, Clone)]
pub enum JavaBridgeReturn {
    Value(JavaValueBridgeReturn),
    Result(JavaResultBridgeReturn),
}

impl JavaBridgeReturn {
    pub fn java_type(&self) -> &str {
        match self {
            Self::Value(return_info) => return_info.java_type.as_str(),
            Self::Result(return_info) => return_info.ok_java_type.as_str(),
        }
    }

    pub fn jni_type(&self) -> &str {
        match self {
            Self::Value(return_info) => return_info.jni_type.as_str(),
            Self::Result(return_info) => return_info.jni_type.as_str(),
        }
    }

    pub fn default_value(&self) -> &str {
        match self {
            Self::Value(return_info) => return_info.default_value.as_str(),
            Self::Result(return_info) => return_info.default_value.as_str(),
        }
    }

    pub fn is_value(&self) -> bool {
        matches!(self, Self::Value(_))
    }

    pub fn is_result(&self) -> bool {
        matches!(self, Self::Result(_))
    }

    pub fn value_return(&self) -> Option<&JavaValueBridgeReturn> {
        match self {
            Self::Value(return_info) => Some(return_info),
            Self::Result(_) => None,
        }
    }

    pub fn result_return(&self) -> Option<&JavaResultBridgeReturn> {
        match self {
            Self::Value(_) => None,
            Self::Result(return_info) => Some(return_info),
        }
    }

    pub fn requires_wire_writer(&self) -> bool {
        match self {
            Self::Value(return_info) => return_info.requires_wire_writer(),
            Self::Result(_) => true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct JavaValueBridgeReturn {
    pub java_type: String,
    pub jni_type: String,
    pub default_value: String,
    pub render: JavaValueBridgeRender,
}

impl JavaValueBridgeReturn {
    pub fn is_encoded(&self) -> bool {
        matches!(self.render, JavaValueBridgeRender::Encode { .. })
    }

    pub fn direct_prefix(&self) -> &str {
        match &self.render {
            JavaValueBridgeRender::Direct { prefix, .. } => prefix,
            JavaValueBridgeRender::Encode { .. } => "",
        }
    }

    pub fn direct_suffix(&self) -> &str {
        match &self.render {
            JavaValueBridgeRender::Direct { suffix, .. } => suffix,
            JavaValueBridgeRender::Encode { .. } => "",
        }
    }

    pub fn encode_size_expr(&self) -> &str {
        match &self.render {
            JavaValueBridgeRender::Direct { .. } => "",
            JavaValueBridgeRender::Encode { size_expr, .. } => size_expr,
        }
    }

    pub fn encode_expr(&self) -> &str {
        match &self.render {
            JavaValueBridgeRender::Direct { .. } => "",
            JavaValueBridgeRender::Encode { encode_expr, .. } => encode_expr,
        }
    }

    pub fn requires_wire_writer(&self) -> bool {
        self.is_encoded()
    }
}

#[derive(Debug, Clone)]
pub enum JavaValueBridgeRender {
    Direct {
        prefix: String,
        suffix: String,
    },
    Encode {
        size_expr: String,
        encode_expr: String,
    },
}

#[derive(Debug, Clone)]
pub struct JavaResultBridgeReturn {
    pub ok_java_type: String,
    pub err_java_type: String,
    pub jni_type: String,
    pub default_value: String,
    pub encode_size_expr: String,
    pub encode_expr: String,
    pub error_capture: JavaCallbackErrorCapture,
}

impl JavaResultBridgeReturn {
    pub fn has_exception_class(&self) -> bool {
        self.error_capture.exception_class.is_some()
    }

    pub fn exception_class(&self) -> &str {
        self.error_capture
            .exception_class
            .as_deref()
            .unwrap_or("RuntimeException")
    }
}

#[derive(Debug, Clone)]
pub struct JavaCallbackErrorCapture {
    pub exception_class: Option<String>,
    pub is_string: bool,
}

#[derive(Debug, Clone)]
pub struct JavaSyncCallbackMethod {
    pub doc: Option<String>,
    pub name: String,
    pub ffi_name: String,
    pub params: Vec<JavaBridgeParam>,
    pub return_info: Option<JavaBridgeReturn>,
    pub proxy: JavaCallbackProxySyncMethod,
}

impl JavaSyncCallbackMethod {
    pub fn requires_wire_writer(&self) -> bool {
        self.return_info
            .as_ref()
            .is_some_and(JavaBridgeReturn::requires_wire_writer)
    }
}

#[derive(Debug, Clone)]
pub struct JavaAsyncCallbackMethod {
    pub doc: Option<String>,
    pub name: String,
    pub ffi_name: String,
    pub params: Vec<JavaBridgeParam>,
    pub return_info: Option<JavaBridgeReturn>,
    pub invoker_suffix: String,
    pub proxy: JavaCallbackProxyAsyncMethod,
}

impl JavaAsyncCallbackMethod {
    pub fn boxed_return_type(&self) -> &str {
        self.return_info
            .as_ref()
            .map(JavaBridgeReturn::java_type)
            .map(box_java_type)
            .unwrap_or("Void")
    }

    pub fn success_invoker_name(&self) -> String {
        format!("invokeAsyncCallback{}", self.invoker_suffix)
    }

    pub fn failure_invoker_name(&self) -> String {
        format!("invokeAsyncCallback{}Failure", self.invoker_suffix)
    }

    pub fn requires_wire_writer(&self) -> bool {
        self.return_info
            .as_ref()
            .is_some_and(JavaBridgeReturn::requires_wire_writer)
    }
}

#[derive(Debug, Clone)]
pub struct JavaAsyncCallbackInvoker {
    pub suffix: String,
    pub result_jni_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct JavaCallbackProxySyncMethod {
    pub native_name: String,
    pub params: Vec<JavaParam>,
    pub return_type: String,
    pub return_plan: JavaReturnPlan,
    pub wire_writers: Vec<JavaWireWriter>,
}

#[derive(Debug, Clone)]
pub struct JavaCallbackProxyAsyncMethod {
    pub native_name: String,
    pub params: Vec<JavaParam>,
    pub return_type: String,
    pub return_plan: JavaReturnPlan,
    pub wire_writers: Vec<JavaWireWriter>,
    pub success_name: String,
    pub failure_name: String,
}

impl JavaCallbackProxyAsyncMethod {
    pub fn boxed_return_type(&self) -> &str {
        box_java_type(&self.return_type)
    }
}

impl JavaAsyncCallbackInvoker {
    pub fn success_name(&self) -> String {
        format!("invokeAsyncCallback{}", self.suffix)
    }

    pub fn failure_name(&self) -> String {
        format!("invokeAsyncCallback{}Failure", self.suffix)
    }

    pub fn has_result(&self) -> bool {
        self.result_jni_type.is_some()
    }

    pub fn result_jni_type(&self) -> &str {
        self.result_jni_type.as_deref().unwrap_or("")
    }
}

fn box_java_type(java_type: &str) -> &str {
    match java_type {
        "void" => "Void",
        "boolean" => "Boolean",
        "byte" => "Byte",
        "short" => "Short",
        "int" => "Integer",
        "long" => "Long",
        "float" => "Float",
        "double" => "Double",
        other => other,
    }
}

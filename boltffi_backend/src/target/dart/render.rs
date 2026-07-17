use boltffi_binding::{
    BuiltinType, CallbackDecl, ClassDecl, ConstantDecl, ConstantValueDecl, DataVariantPayload,
    DefaultValue, DirectValueType, EnumDecl, ErrorChannel, ExecutionDecl, FunctionDecl,
    HandlePresence, HandleTarget, ImportedCallable, Native, OutgoingParam, ParamPlan, RecordDecl,
    ReturnPlan, StreamDecl, StreamItemPlan, StreamMode, TypeRef,
};

use crate::{
    bridge::c::CBridgeContract,
    core::{Emitted, Error, RenderContext, Result},
};

use super::{
    DartHost, call,
    codec::{Reader, Writer},
    ffi, name_style, primitive,
};

pub fn dart_type(ty: &TypeRef, context: &RenderContext<Native>) -> Result<String> {
    Ok(match ty {
        TypeRef::Primitive(value) => primitive::api_type(*value)?.to_owned(),
        TypeRef::String | TypeRef::InternedString { .. } => "String".to_owned(),
        TypeRef::Bytes => "$$typed_data.Uint8List".to_owned(),
        TypeRef::Record(id) => context
            .record(*id)
            .map(|decl| name_style::upper_camel(decl.name()))
            .ok_or(missing("record"))?,
        TypeRef::Enum(id) => context
            .enumeration(*id)
            .map(|decl| name_style::upper_camel(decl.name()))
            .ok_or(missing("enum"))?,
        TypeRef::Class(id) => context
            .class(*id)
            .map(|decl| name_style::upper_camel(decl.name()))
            .ok_or(missing("class"))?,
        TypeRef::Callback(id) => context
            .callback(*id)
            .map(|decl| name_style::upper_camel(decl.name()))
            .ok_or(missing("callback"))?,
        TypeRef::Custom(id) => {
            if let Some(mapping) = context.custom_type_mapping(*id) {
                mapping.target_type().as_str().to_owned()
            } else {
                context
                    .custom_type(*id)
                    .map(|decl| name_style::upper_camel(decl.name()))
                    .ok_or(missing("custom type"))?
            }
        }
        TypeRef::Builtin(value) => match value {
            BuiltinType::Duration => "Duration".to_owned(),
            BuiltinType::SystemTime => "DateTime".to_owned(),
            BuiltinType::Uuid | BuiltinType::Url => "String".to_owned(),
        },
        TypeRef::Optional(inner) => format!("{}?", dart_type(inner, context)?),
        TypeRef::Sequence(inner) => format!("List<{}>", dart_type(inner, context)?),
        TypeRef::Tuple(elements) => format!(
            "({})",
            elements
                .iter()
                .map(|element| dart_type(element, context))
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        ),
        TypeRef::Result { ok, err } => format!(
            "BoltFFIResult<{}, {}>",
            dart_type(ok, context)?,
            dart_type(err, context)?
        ),
        TypeRef::Map { key, value } => format!(
            "Map<{}, {}>",
            dart_type(key, context)?,
            dart_type(value, context)?
        ),
        _ => return unsupported("unknown Dart type reference"),
    })
}

pub fn custom_type(
    decl: &boltffi_binding::CustomTypeDecl,
    context: &RenderContext<Native>,
) -> Result<Emitted> {
    let name = name_style::upper_camel(decl.name());
    if let Some(mapping) = context.custom_type_mapping(decl.id()) {
        // Mapped types use the configured public type; conversion happens at
        // the wire boundary via the representation (string for uuid/url).
        let target = mapping.target_type().as_str();
        return Ok(Emitted::primary(format!("typedef {name} = {target};\n\n")));
    }
    Ok(Emitted::primary(format!(
        "typedef {name} = {};\n\n",
        dart_type(decl.representation(), context)?
    )))
}

pub fn stream(
    decl: &StreamDecl<Native>,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Emitted> {
    let protocol = bridge
        .source_stream(decl.id())
        .ok_or(Error::BrokenBridgeContract {
            bridge: DartHost::TARGET,
            invariant: "missing C stream protocol for Dart stream",
        })?;
    let (item_type, read_batch) = match decl.item() {
        StreamItemPlan::Direct { ty, .. } => {
            let public = call::direct_type(ty, context)?;
            let batch = protocol.direct_batch().ok_or(Error::BrokenBridgeContract {
                bridge: DartHost::TARGET,
                invariant: "missing direct Dart stream batch",
            })?;
            let native = ffi::native_type(batch.item());
            let decode = match ty {
                DirectValueType::Primitive(_) => "(raw + i).value".to_owned(),
                DirectValueType::Enum(_) => format!("{public}._fromValue((raw + i).value)"),
                DirectValueType::Record(_) => {
                    format!("{public}._fromStruct((raw + i).ref)")
                }
                _ => return unsupported("unknown direct Dart stream item"),
            };
            (
                public,
                format!(
                    "if (maxCount == 0) return const [];\n  final raw = $$extffi.calloc<{native}>(maxCount);\n  try {{\n    final count = _f${}(subscription, raw, maxCount);\n    return List.generate(count, (i) => {decode}, growable: false);\n  }} finally {{\n    $$extffi.calloc.free(raw);\n  }}",
                    protocol.pop_batch().name()
                ),
            )
        }
        StreamItemPlan::Encoded { ty, read, .. } => {
            let public = dart_type(ty, context)?;
            let decode = read.render_with(&mut Reader::new("reader", context))?;
            (
                public,
                format!(
                    "final buffer = _f${}(subscription, maxCount);\n  try {{\n    if (buffer.ptr == $$ffi.nullptr || buffer.len == 0) return const [];\n    final reader = _$$WireReader(buffer.ptr, buffer.len);\n    final count = reader.readU32();\n    return List.generate(count, (_) => {decode}, growable: false);\n  }} finally {{\n    if (buffer.ptr != $$ffi.nullptr) _f$boltffi_free_buf(buffer);\n  }}",
                    protocol.pop_batch().name()
                ),
            )
        }
        _ => return unsupported("unknown Dart stream item plan"),
    };
    let name = name_style::lower_camel(decl.name());
    let type_name = name_style::upper_camel(decl.name());
    let subscribe_args = if decl.owner().is_some() {
        "_rawHandle"
    } else {
        ""
    };
    let read_name = format!("_read{type_name}Batch");
    let common = format!(
        "List<{item_type}> {read_name}(int subscription, int maxCount) {{\n  {}\n}}",
        indent(&read_batch, 2)
    );
    let body = match decl.mode() {
        StreamMode::Async => format!(
            "$$async.Stream<{item_type}> {name}() {{\n  late final _$$BoltFFIStreamPump<{item_type}> pump;\n  late final $$async.StreamController<{item_type}> controller;\n  controller = $$async.StreamController<{item_type}>(\n    onListen: () {{\n      final subscription = _f${}({subscribe_args});\n      pump = _$$BoltFFIStreamPump<{item_type}>(\n        subscription: subscription,\n        readBatch: {read_name},\n        poll: _f${},\n        unsubscribe: _f${},\n        free: _f${},\n        onItem: controller.add,\n        onDone: controller.close,\n      )..start();\n    }},\n    onCancel: () => pump.cancel(),\n  );\n  return controller.stream;\n}}",
            protocol.subscribe().name(),
            protocol.poll().name(),
            protocol.unsubscribe().name(),
            protocol.free().name(),
        ),
        StreamMode::Batch => {
            let subscription = format!("{type_name}Subscription");
            let free = protocol.free().name();
            let unsubscribe = protocol.unsubscribe().name();
            format!(
                "{subscription} {name}() => {subscription}._(\n  _f${}({subscribe_args}),\n);\n\nfinal class {subscription} {{\n  static final _finalizer = Finalizer<int>((handle) {{\n    if (handle != 0) _f${free}(handle);\n  }});\n  int _handle;\n  bool _closed = false;\n  {subscription}._(this._handle) {{\n    if (_handle != 0) _finalizer.attach(this, _handle, detach: this);\n  }}\n\n  List<{item_type}> popBatch({{int maxCount = 16}}) {{\n    if (_closed || _handle == 0) return const [];\n    return {read_name}(_handle, maxCount);\n  }}\n\n  int wait(int timeoutMilliseconds) =>\n      _closed || _handle == 0 ? -1 : _f${}(_handle, timeoutMilliseconds);\n\n  void unsubscribe() {{\n    if (_closed || _handle == 0) return;\n    _f${unsubscribe}(_handle);\n    _closed = true;\n  }}\n\n  void dispose() {{\n    if (_handle == 0) return;\n    _finalizer.detach(this);\n    if (!_closed) _f${unsubscribe}(_handle);\n    _f${free}(_handle);\n    _closed = true;\n    _handle = 0;\n  }}\n}}",
                protocol.subscribe().name(),
                protocol.wait().name(),
            )
        }
        StreamMode::Callback => format!(
            "BoltFFIStreamCancellation {name}(void Function({item_type}) callback) {{\n  final subscription = _f${}({subscribe_args});\n  final pump = _$$BoltFFIStreamPump<{item_type}>(\n    subscription: subscription,\n    readBatch: {read_name},\n    poll: _f${},\n    unsubscribe: _f${},\n    free: _f${},\n    onItem: callback,\n    onDone: () {{}},\n  )..start();\n  return BoltFFIStreamCancellation(pump.cancel);\n}}",
            protocol.subscribe().name(),
            protocol.poll().name(),
            protocol.unsubscribe().name(),
            protocol.free().name(),
        ),
        _ => return unsupported("unknown Dart stream mode"),
    };
    let body = if let Some(owner) = decl.owner() {
        let owner = context
            .class(owner)
            .map(|class| name_style::upper_camel(class.name()))
            .ok_or(missing("Dart stream owner"))?;
        if let Some((method, class)) = body.split_once("\n\nfinal class") {
            format!(
                "extension {type_name}Stream on {owner} {{\n{}\n}}\n\nfinal class{}",
                indent(method, 2),
                class
            )
        } else {
            format!(
                "extension {type_name}Stream on {owner} {{\n{}\n}}",
                indent(&body, 2)
            )
        }
    } else {
        body
    };
    Ok(Emitted::primary(format!("{common}\n\n{body}\n\n")))
}

pub fn record(
    decl: &RecordDecl<Native>,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Emitted> {
    let name = name_style::upper_camel(decl.name());
    let exception = if decl.is_error_payload() {
        " implements Exception"
    } else {
        ""
    };
    let (fields, reads, writes, direct_bridge) = match decl {
        RecordDecl::Direct(record) => {
            let fields = record
                .fields()
                .iter()
                .map(|field| {
                    Ok((
                        name_style::field(field.key()),
                        primitive::direct_field(field.ty())?,
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            let reads = record
                .fields()
                .iter()
                .map(|field| {
                    Ok(format!(
                        "reader.read{}()",
                        primitive::wire_suffix(field.ty().primitive())?
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            let writes = record
                .fields()
                .iter()
                .map(|field| {
                    Ok(format!(
                        "writer.write{}({});",
                        primitive::wire_suffix(field.ty().primitive())?,
                        name_style::field(field.key())
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            let c_record =
                bridge
                    .source_direct_record(record.id())
                    .ok_or(Error::BrokenBridgeContract {
                        bridge: DartHost::TARGET,
                        invariant: "missing C record for direct Dart record",
                    })?;
            let c_name = ffi::record_name(c_record);
            let assignments = record
                .fields()
                .iter()
                .zip(c_record.fields())
                .map(|(field, c_field)| {
                    format!(
                        "      ..{} = {}",
                        c_field.name(),
                        name_style::field(field.key())
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let from_fields = record
                .fields()
                .iter()
                .zip(c_record.fields())
                .map(|(field, c_field)| {
                    format!(
                        "      {}: value.{},",
                        name_style::field(field.key()),
                        c_field.name()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            (
                fields,
                reads,
                writes,
                format!(
                    "\n  {c_name} _toStruct() => $$ffi.Struct.create<{c_name}>()\n{assignments};\n\n  factory {name}._fromStruct({c_name} value) => {name}(\n{from_fields}\n  );\n"
                ),
            )
        }
        RecordDecl::Encoded(record) => {
            let fields = record
                .fields()
                .iter()
                .map(|field| {
                    Ok((
                        name_style::field(field.key()),
                        dart_type(field.ty(), context)?,
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            let reads = record
                .fields()
                .iter()
                .map(|field| {
                    field
                        .read()
                        .render_with(&mut Reader::new("reader", context))
                })
                .collect::<Result<Vec<_>>>()?;
            let writes = record
                .fields()
                .iter()
                .flat_map(|field| {
                    field
                        .write()
                        .render_with(&mut Writer::new("writer", "", context))
                })
                .collect::<Result<Vec<_>>>()?;
            (fields, reads, writes, String::new())
        }
        _ => return unsupported("unknown record declaration"),
    };
    let declarations = fields
        .iter()
        .map(|(field, ty)| format!("  final {ty} {field};"))
        .collect::<Vec<_>>()
        .join("\n");
    let parameters = fields
        .iter()
        .map(|(field, _)| format!("required this.{field}"))
        .collect::<Vec<_>>()
        .join(", ");
    let decode = fields
        .iter()
        .zip(reads)
        .map(|((field, _), read)| format!("      {field}: {read},"))
        .collect::<Vec<_>>()
        .join("\n");
    let encode = writes
        .into_iter()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Emitted::primary(format!(
        "final class {name}{exception} {{\n{declarations}\n\n  const {name}({{{parameters}}});\n\n  factory {name}._decode(_$$WireReader reader) => {name}(\n{decode}\n  );\n\n  void _encode(_$$WireWriter writer) {{\n{encode}\n  }}\n{direct_bridge}}}\n\n"
    )))
}

pub fn enumeration(
    decl: &EnumDecl<Native>,
    _: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Emitted> {
    match decl {
        EnumDecl::CStyle(value) => {
            let name = name_style::upper_camel(value.name());
            let variants = value
                .variants()
                .iter()
                .map(|variant| {
                    format!(
                        "  static const {} = {name}._({}, '{}');",
                        name_style::lower_camel(variant.name()),
                        variant.discriminant().get(),
                        name_style::lower_camel(variant.name())
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let values = value
                .variants()
                .iter()
                .map(|variant| name_style::lower_camel(variant.name()))
                .collect::<Vec<_>>()
                .join(", ");
            Ok(Emitted::primary(format!(
                "final class {name}{} {{\n  final int value;\n  final String name;\n  const {name}._(this.value, this.name);\n\n{variants}\n  static const values = <{name}>[{values}];\n  static {name} _fromValue(int value) => values.firstWhere((item) => item.value == value, orElse: () => throw StateError('Unknown {name} value: $value'));\n  void _encode(_$$WireWriter writer) => writer.writeI32(value);\n  static {name} _decode(_$$WireReader reader) => _fromValue(reader.readI32());\n  @override String toString() => name;\n}}\n\n",
                if value.is_error_payload() {
                    " implements Exception"
                } else {
                    ""
                }
            )))
        }
        EnumDecl::Data(value) => data_enum(value, context),
        _ => unsupported("unknown enum declaration"),
    }
}

pub fn function(
    decl: &FunctionDecl<Native>,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Emitted> {
    let callable = decl.callable();
    let params = exported_parameters(callable, context)?;
    let return_ty = call::exported_api_return(callable, context)?;
    let function = call::c_function(decl.symbol(), bridge)?;
    let (setup, cleanup, args, mut helpers) =
        call::marshal_exported(callable, None, bridge, context, function)?;
    let body = match callable.execution() {
        ExecutionDecl::Synchronous(_) => {
            let (body, mut more) =
                sync_exported_call(callable, function, args.clone(), bridge, context)?;
            helpers.append(&mut more);
            body
        }
        ExecutionDecl::Asynchronous(protocol) => {
            let invocation = format!("_f${}({})", function.name(), args.join(", "));
            async_exported_return(callable, protocol, invocation, bridge, context)?
        }
        _ => return unsupported("unknown free function execution"),
    };
    let body = wrap_call(setup, cleanup, body);
    Ok(super::closure::attach_helpers(
        Emitted::primary(format!(
            "{return_ty} {}({params}) {{\n  {}\n}}\n\n",
            name_style::lower_camel(decl.name()),
            indent(&body, 2)
        )),
        helpers,
    ))
}

pub fn constant(
    decl: &ConstantDecl<Native>,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Emitted> {
    let name = name_style::lower_camel(decl.name());
    let source = match decl.value() {
        ConstantValueDecl::Inline { ty, value, .. } => format!(
            "const {} {name} = {};\n\n",
            dart_type(ty, context)?,
            default_value(ty, value, context)?
        ),
        ConstantValueDecl::Accessor { symbol, callable } => {
            if callable.execution().uses_async_execution() || !callable.params().is_empty() {
                return unsupported("Dart constant accessor shape");
            }
            let return_ty = call::return_type(callable.returns().plan(), context)?;
            let function = call::c_function(symbol, bridge)?;
            let invocation = format!("_f${}()", function.name());
            let body = sync_exported_return(
                callable.returns().plan(),
                callable.error().channel(),
                &invocation,
                bridge,
                context,
            )?;
            format!("{return_ty} get {name} {{\n  {}\n}}\n\n", indent(&body, 2))
        }
        _ => return unsupported("unknown Dart constant value"),
    };
    Ok(Emitted::primary(source))
}

fn default_value(
    ty: &TypeRef,
    value: &DefaultValue,
    context: &RenderContext<Native>,
) -> Result<String> {
    Ok(match value {
        DefaultValue::Bool(value) => value.to_string(),
        DefaultValue::Integer(value) => value.get().to_string(),
        DefaultValue::Float(value) => {
            let value = value.to_f64();
            if value.is_nan() {
                "double.nan".to_owned()
            } else if value == f64::INFINITY {
                "double.infinity".to_owned()
            } else if value == f64::NEG_INFINITY {
                "double.negativeInfinity".to_owned()
            } else if value == 0.0 && value.is_sign_negative() {
                "-0.0".to_owned()
            } else {
                format!("{value:?}")
            }
        }
        DefaultValue::String(value) => dart_string(value),
        DefaultValue::EnumVariant { variant_name, .. } => match ty {
            TypeRef::Enum(id) => {
                let enumeration = context.enumeration(*id).ok_or(missing("constant enum"))?;
                format!(
                    "{}.{}",
                    name_style::upper_camel(enumeration.name()),
                    name_style::lower_camel(variant_name)
                )
            }
            _ => return unsupported("Dart enum constant type"),
        },
        DefaultValue::Null => "null".to_owned(),
        _ => return unsupported("unknown Dart constant literal"),
    })
}

fn dart_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('$', "\\$")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("'{escaped}'")
}

pub fn class(
    decl: &ClassDecl<Native>,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Emitted> {
    let name = name_style::upper_camel(decl.name());
    let release = call::c_function(decl.release(), bridge)?.name();
    let mut helpers = Vec::new();
    let initializers = decl
        .initializers()
        .iter()
        .map(|initializer| {
            let callable = initializer.callable();
            let params = exported_parameters(callable, context)?;
            let function = call::c_function(initializer.symbol(), bridge)?;
            let (setup, cleanup, args, mut closure_helpers) =
                call::marshal_exported(callable, None, bridge, context, function)?;
            helpers.append(&mut closure_helpers);
            let body = initializer_body(callable, function, args, &name, context)?;
            let body = wrap_call(setup, cleanup, body);
            let initializer_name = name_style::lower_camel(initializer.name());
            let constructor = if initializer_name == "new" || initializer_name == "new_" {
                name.clone()
            } else {
                format!("{name}.{initializer_name}")
            };
            Ok(format!(
                "  factory {constructor}({params}) {{\n    {}\n  }}",
                indent(&body, 4)
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join("\n\n");
    let methods = decl
        .methods()
        .iter()
        .map(|method| {
            let callable = method.callable();
            let params = exported_parameters(callable, context)?;
            let return_ty = call::exported_api_return(callable, context)?;
            let function = call::c_function(method.target(), bridge)?;
            // Static methods have no receiver in the C ABI; only instance methods do.
            let receiver = callable.receiver().map(|_| "_rawHandle");
            let (setup, cleanup, args, mut closure_helpers) =
                call::marshal_exported(callable, receiver, bridge, context, function)?;
            helpers.append(&mut closure_helpers);
            let body = match callable.execution() {
                ExecutionDecl::Synchronous(_) => {
                    let (body, mut more) =
                        sync_exported_call(callable, function, args.clone(), bridge, context)?;
                    helpers.append(&mut more);
                    body
                }
                ExecutionDecl::Asynchronous(protocol) => {
                    let invocation = format!("_f${}({})", function.name(), args.join(", "));
                    async_exported_return(callable, protocol, invocation, bridge, context)?
                }
                _ => return unsupported("unknown class method execution"),
            };
            let body = wrap_call(setup, cleanup, body);
            let qualifier = if callable.receiver().is_none() {
                "static "
            } else {
                ""
            };
            Ok(format!(
                "  {qualifier}{return_ty} {}({params}) {{\n    {}\n  }}",
                name_style::lower_camel(method.name()),
                indent(&body, 4)
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join("\n\n");
    Ok(super::closure::attach_helpers(
        Emitted::primary(format!(
            "final class {name} {{\n  static final _finalizer = Finalizer<int>((handle) => _f${release}(handle));\n  int _handle;\n  bool _closed = false;\n  {name}._(this._handle) {{ _finalizer.attach(this, _handle, detach: this); }}\n\n  int get _rawHandle {{\n    if (_closed) throw StateError('{name} has been disposed');\n    return _handle;\n  }}\n\n{initializers}\n\n{methods}\n\n  void dispose() {{\n    if (_closed) return;\n    _closed = true;\n    _finalizer.detach(this);\n    _f${release}(_handle);\n    _handle = 0;\n  }}\n}}\n\n"
        )),
        helpers,
    ))
}

fn initializer_body(
    callable: &boltffi_binding::ExportedCallable<Native>,
    function: &crate::bridge::c::Function,
    mut args: Vec<String>,
    class_name: &str,
    context: &RenderContext<Native>,
) -> Result<String> {
    match callable.error().channel() {
        ErrorChannel::None => Ok(format!(
            "final handle = _f${}({});\nreturn {class_name}._(handle);",
            function.name(),
            args.join(", ")
        )),
        ErrorChannel::Encoded { ty, codec, .. } => {
            let success = function
                .parameter_groups()
                .iter()
                .find_map(|group| match group {
                    crate::bridge::c::ParameterGroup::SuccessOut(index) => Some(*index),
                    _ => None,
                })
                .ok_or(Error::BrokenBridgeContract {
                    bridge: DartHost::TARGET,
                    invariant: "fallible Dart initializer has no success output",
                })?;
            let crate::bridge::c::Type::MutPointer(inner) = function.parameter(success).ty() else {
                return Err(Error::BrokenBridgeContract {
                    bridge: DartHost::TARGET,
                    invariant: "fallible Dart initializer success output is not a pointer",
                });
            };
            args.push("success".to_owned());
            let decode = codec.render_with(&mut Reader::new("errorReader", context))?;
            let thrown = if matches!(ty, TypeRef::String) {
                format!("_$$FFIException(-1, {decode})")
            } else {
                decode
            };
            Ok(format!(
                "final success = $$extffi.calloc<{}>();\ntry {{\n  final error = _f${}({});\n  if (error.ptr != $$ffi.nullptr) {{\n    try {{\n      final errorReader = _$$WireReader(error.ptr, error.len);\n      throw {thrown};\n    }} finally {{ _f$boltffi_free_buf(error); }}\n  }}\n  return {class_name}._(success.value);\n}} finally {{\n  $$extffi.calloc.free(success);\n}}",
                ffi::native_type(inner),
                function.name(),
                args.join(", ")
            ))
        }
        _ => unsupported("Dart initializer error channel"),
    }
}

pub fn callback(
    decl: &CallbackDecl<Native>,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Emitted> {
    let protocol = bridge
        .source_callback(decl.id())
        .ok_or(Error::BrokenBridgeContract {
            bridge: DartHost::TARGET,
            invariant: "missing Dart callback C protocol",
        })?;
    let name = name_style::upper_camel(decl.name());
    let source_methods = decl.protocol().vtable().methods();
    if source_methods.len() != protocol.methods().len() {
        return Err(Error::BrokenBridgeContract {
            bridge: DartHost::TARGET,
            invariant: "Dart callback method count mismatch",
        });
    }
    let interface = source_methods
        .iter()
        .map(|method| {
            let params = imported_parameters(method.callable(), context)?;
            let returns = call::callback_api_return(method.callable(), context)?;
            Ok(format!(
                "  {returns} {}({params});",
                name_style::lower_camel(method.name())
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join("\n\n");
    let implementations = source_methods
        .iter()
        .zip(protocol.methods())
        .map(|(method, slot)| callback_method(&name, method, slot, bridge, context))
        .collect::<Result<Vec<_>>>()?
        .join("\n\n");
    let vtable_name = ffi::record_name(protocol.vtable());
    let assignments = protocol
        .methods()
        .iter()
        .map(|slot| {
            format!(
                "    _vtable.ref.{} = $$ffi.Pointer.fromFunction(_I${name}.{}{});",
                slot.name().as_str(),
                slot.name().as_str(),
                callback_exceptional_return(slot.returns())
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let foreign = super::foreign::proxy_class(decl, bridge, context)?.unwrap_or_default();
    Ok(Emitted::primary(format!(
        "abstract interface class {name} {{\n{interface}\n}}\n\nfinal class _I${name} {{\n  static void free(int handle) => _I${name}HandleMap.remove(handle);\n  static int clone(int handle) => _I${name}HandleMap.clone(handle);\n\n{implementations}\n}}\n\nfinal class _I${name}HandleMapImpl {{\n  final Map<int, {name}> _map = {{}};\n  int _counter = 1;\n  late final $$ffi.Pointer<{vtable_name}> _vtable;\n\n  _I${name}HandleMapImpl() {{\n    _vtable = $$extffi.calloc<{vtable_name}>();\n    _vtable.ref.free = $$ffi.Pointer.fromFunction(_I${name}.free);\n    _vtable.ref.clone = $$ffi.Pointer.fromFunction(_I${name}.clone, 0);\n{assignments}\n    _f${}(_vtable);\n  }}\n\n  int insert({name} value) {{ final handle = _counter += 2; _map[handle] = value; return handle; }}\n  {name}? get(int handle) => _map[handle];\n  {name}? remove(int handle) => _map.remove(handle);\n  int clone(int handle) {{ final value = _map[handle]; return value == null ? 0 : insert(value); }}\n  _$$BoltFFICallbackHandle createHandle({name} value) => _f${}(insert(value));\n}}\n\nfinal _I${name}HandleMap = _I${name}HandleMapImpl();\n\n{foreign}",
        protocol.register().name(),
        protocol.create_handle().name()
    )))
}

fn callback_method(
    callback_name: &str,
    method: &boltffi_binding::ImportedMethodDecl<Native, boltffi_binding::VTableSlot>,
    slot: &crate::bridge::c::CallbackSlot,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    let callable = method.callable();
    let signature = slot
        .parameters()
        .iter()
        .map(|parameter| format!("{} {}", ffi::dart_type(parameter.ty()), parameter.name()))
        .collect::<Vec<_>>()
        .join(", ");
    let mut setup = vec![
        format!("final impl = _I${callback_name}HandleMap.get(handle);"),
        "if (impl == null) { throw StateError('released BoltFFI callback handle'); }".into(),
    ];
    let mut args = Vec::new();
    for (param, group) in callable.params().iter().zip(slot.source_parameter_groups()) {
        let OutgoingParam::Value(plan) = param.payload() else {
            return unsupported("callback closure parameter");
        };
        let local = name_style::lower_camel(param.name());
        let expression = match (plan, group) {
            (ParamPlan::Direct { ty, .. }, crate::bridge::c::ParameterGroup::Value(index)) => {
                let raw = slot.parameter(*index).name();
                match ty {
                    DirectValueType::Primitive(boltffi_binding::Primitive::Bool) => {
                        format!("{raw}")
                    }
                    DirectValueType::Enum(id) => format!(
                        "{}._fromValue({raw})",
                        call::direct_type(&DirectValueType::Enum(*id), context)?
                    ),
                    DirectValueType::Record(_) => {
                        format!("{}._fromStruct({raw})", call::direct_type(ty, context)?)
                    }
                    _ => raw.to_owned(),
                }
            }
            (
                ParamPlan::Encoded { codec, .. },
                crate::bridge::c::ParameterGroup::ByteSlice(bytes),
            ) => {
                let ptr = slot.parameter(bytes.pointer()).name();
                let len = slot.parameter(bytes.length()).name();
                let reader = format!("{local}Reader");
                setup.push(format!("final {reader} = _$$WireReader({ptr}, {len});"));
                codec.render_with(&mut Reader::new(&reader, context))?
            }
            (
                ParamPlan::ScalarOption { primitive },
                crate::bridge::c::ParameterGroup::ByteSlice(bytes),
            ) => {
                // Native callback args use an empty buffer for None and a
                // wire-encoded Option payload for Some (see boltffi_macros).
                let ptr = slot.parameter(bytes.pointer()).name();
                let len = slot.parameter(bytes.length()).name();
                let suffix = primitive::wire_suffix(*primitive)?;
                format!(
                    "{len} == 0 ? null : _$$WireReader({ptr}, {len}).readOptional((reader) => reader.read{suffix}())"
                )
            }
            (
                ParamPlan::DirectVec { element, .. },
                crate::bridge::c::ParameterGroup::DirectVector(vector),
            ) => {
                let ptr = slot.parameter(vector.pointer()).name();
                let len = slot.parameter(vector.length()).name();
                match element {
                    boltffi_binding::DirectVectorElementType::Primitive(primitive) => {
                        let native = call::primitive_native_type(primitive.primitive())?;
                        format!(
                            "List.generate({len}, (i) => (({ptr}.cast<{native}>()) + i).value, growable: false)"
                        )
                    }
                    boltffi_binding::DirectVectorElementType::Record(id) => {
                        let c_record = bridge.source_direct_record(*id).ok_or(
                            Error::BrokenBridgeContract {
                                bridge: DartHost::TARGET,
                                invariant: "missing callback direct vector record",
                            },
                        )?;
                        let c_name = ffi::record_name(c_record);
                        let public = context
                            .record(*id)
                            .map(|record| name_style::upper_camel(record.name()))
                            .ok_or(missing("callback direct vector record"))?;
                        format!(
                            "List.generate({len} ~/ $$ffi.sizeOf<{c_name}>(), (i) => {public}._fromStruct((({ptr}.cast<{c_name}>()) + i).ref), growable: false)"
                        )
                    }
                    _ => return unsupported("unknown callback direct vector"),
                }
            }
            (
                ParamPlan::Handle {
                    target, presence, ..
                },
                crate::bridge::c::ParameterGroup::Value(index),
            ) => {
                let raw = slot.parameter(*index).name();
                let decoded = match target {
                    HandleTarget::Class(id) => {
                        let name = context
                            .class(*id)
                            .map(|class| name_style::upper_camel(class.name()))
                            .ok_or(missing("callback class parameter"))?;
                        format!("{name}._({raw})")
                    }
                    _ => return unsupported("callback handle parameter target"),
                };
                if *presence == HandlePresence::Nullable {
                    format!("{raw} == 0 ? null : {decoded}")
                } else {
                    decoded
                }
            }
            _ => return unsupported("callback parameter marshalling shape"),
        };
        setup.push(format!("final {local}Decoded = {expression};"));
        args.push(format!("{local}Decoded"));
    }
    let call = format!(
        "impl.{}({})",
        name_style::lower_camel(method.name()),
        args.join(", ")
    );
    let body = match callable.execution() {
        ExecutionDecl::Synchronous(_) => {
            callback_sync_return(callable, slot, &call, bridge, context)?
        }
        ExecutionDecl::Asynchronous(_) => callback_async_return(callable, slot, &call, context)?,
        _ => return unsupported("unknown callback execution"),
    };
    setup.push(body);
    Ok(format!(
        "  static {} {}({signature}){} {{\n    {}\n  }}",
        ffi::dart_type(slot.returns()),
        slot.name().as_str(),
        if callable.execution().uses_async_execution() {
            " async"
        } else {
            ""
        },
        indent(&setup.join("\n"), 4)
    ))
}

fn callback_sync_return(
    callable: &ImportedCallable<Native>,
    slot: &crate::bridge::c::CallbackSlot,
    call_expr: &str,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    match callable.error().channel() {
        ErrorChannel::None => {
            callback_infallible_sync_return(callable.returns().plan(), call_expr, bridge, context)
        }
        ErrorChannel::Encoded { codec, .. } => {
            let out = slot
                .parameter_groups()
                .iter()
                .find_map(|group| match group {
                    crate::bridge::c::ParameterGroup::SuccessOut(index) => {
                        Some(slot.parameter(*index).name())
                    }
                    _ => None,
                });
            let success =
                callback_success_store(callable.returns().plan(), "value", out, bridge, context)?;
            let error = codec
                .render_with(&mut Writer::new("writer", "value", context))
                .into_iter()
                .collect::<Result<Vec<_>>>()?
                .join(" ");
            Ok(format!(
                "final result = {call_expr};\nswitch (result) {{\n  case BoltFFIResult$Ok(:final value):\n    {success}\n    return _$$emptyBuf();\n  case BoltFFIResult$Err(:final value):\n    final writer = _$$WireWriter();\n    try {{\n      {error}\n      return writer.toRustBuffer();\n    }} finally {{ writer.close(); }}\n}}"
            ))
        }
        ErrorChannel::Status => unsupported("Dart synchronous callback status error"),
        _ => unsupported("unknown synchronous callback error"),
    }
}

fn callback_infallible_sync_return(
    plan: &ReturnPlan<Native, boltffi_binding::IntoRust>,
    call_expr: &str,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    Ok(match plan {
        ReturnPlan::Void => format!("{call_expr};"),
        ReturnPlan::DirectViaReturnSlot { ty } => match ty {
            DirectValueType::Primitive(_) => format!("return {call_expr};"),
            DirectValueType::Enum(_) => format!("return {call_expr}.value;"),
            DirectValueType::Record(_) => format!("return {call_expr}._toStruct();"),
            _ => return unsupported("callback direct return"),
        },
        ReturnPlan::EncodedViaReturnSlot { codec, .. } => {
            callback_buffer_return(codec, call_expr, context)?
        }
        ReturnPlan::ScalarOptionViaReturnSlot { primitive } => {
            let suffix = primitive::wire_suffix(*primitive)?;
            format!(
                "final value = {call_expr};\nfinal writer = _$$WireWriter();\ntry {{\n  writer.writeOptional(value, (value, writer) => writer.write{suffix}(value));\n  return writer.toRustBuffer();\n}} finally {{ writer.close(); }}"
            )
        }
        ReturnPlan::DirectVecViaReturnSlot { element } => {
            format!(
                "{}\nreturn buffer;",
                callback_vector_buffer(call_expr, element, bridge, context)?
            )
        }
        ReturnPlan::HandleViaReturnSlot {
            target, presence, ..
        } => match target {
            HandleTarget::Class(_) => {
                if *presence == HandlePresence::Nullable {
                    format!("return {call_expr}?._rawHandle ?? 0;")
                } else {
                    format!("return {call_expr}._rawHandle;")
                }
            }
            HandleTarget::Callback(id) => {
                let callback = context
                    .callback(*id)
                    .ok_or(missing("callback return type"))?;
                let map = format!("_I${}HandleMap", name_style::upper_camel(callback.name()));
                if *presence == HandlePresence::Nullable {
                    format!(
                        "final value = {call_expr};\nreturn value == null ? _$$nullCallbackHandle() : {map}.createHandle(value);"
                    )
                } else {
                    format!("return {map}.createHandle({call_expr});")
                }
            }
            _ => return unsupported("callback handle return target"),
        },
        _ => return unsupported("synchronous callback return shape"),
    })
}

fn callback_buffer_return(
    codec: &boltffi_binding::WritePlan,
    call_expr: &str,
    context: &RenderContext<Native>,
) -> Result<String> {
    let writes = codec
        .render_with(&mut Writer::new("writer", "value", context))
        .into_iter()
        .collect::<Result<Vec<_>>>()?
        .join(" ");
    Ok(format!(
        "final value = {call_expr};\nfinal writer = _$$WireWriter();\ntry {{\n  {writes}\n  return writer.toRustBuffer();\n}} finally {{ writer.close(); }}"
    ))
}

fn callback_success_store(
    plan: &ReturnPlan<Native, boltffi_binding::IntoRust>,
    value: &str,
    out: Option<&str>,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    if matches!(plan, ReturnPlan::Void) {
        return Ok(String::new());
    }
    let out = out.ok_or(Error::BrokenBridgeContract {
        bridge: DartHost::TARGET,
        invariant: "fallible callback success output missing",
    })?;
    Ok(match plan {
        ReturnPlan::DirectViaOutPointer { ty } => match ty {
            DirectValueType::Primitive(_) => format!("{out}.value = {value};"),
            DirectValueType::Enum(_) => format!("{out}.value = {value}.value;"),
            DirectValueType::Record(_) => format!("{out}.ref = {value}._toStruct();"),
            _ => return unsupported("fallible callback direct success"),
        },
        ReturnPlan::EncodedViaOutPointer { codec, .. } => {
            let writes = codec
                .render_with(&mut Writer::new("writer", value, context))
                .into_iter()
                .collect::<Result<Vec<_>>>()?
                .join(" ");
            format!(
                "final writer = _$$WireWriter();\n    try {{ {writes} {out}.ref = writer.toRustBuffer(); }} finally {{ writer.close(); }}"
            )
        }
        ReturnPlan::HandleViaOutPointer {
            target, presence, ..
        } => match target {
            HandleTarget::Class(_) => {
                if *presence == HandlePresence::Nullable {
                    format!("{out}.value = {value}?._rawHandle ?? 0;")
                } else {
                    format!("{out}.value = {value}._rawHandle;")
                }
            }
            HandleTarget::Callback(id) => {
                let callback = context
                    .callback(*id)
                    .ok_or(missing("callback success type"))?;
                let map = format!("_I${}HandleMap", name_style::upper_camel(callback.name()));
                if *presence == HandlePresence::Nullable {
                    format!(
                        "{out}.ref = {value} == null ? _$$nullCallbackHandle() : {map}.createHandle({value});"
                    )
                } else {
                    format!("{out}.ref = {map}.createHandle({value});")
                }
            }
            _ => return unsupported("fallible callback handle success"),
        },
        ReturnPlan::ScalarOptionViaReturnSlot { primitive } => {
            let suffix = primitive::wire_suffix(*primitive)?;
            format!(
                "final writer = _$$WireWriter();\n    try {{ writer.writeOptional({value}, (value, writer) => writer.write{suffix}(value)); {out}.ref = writer.toRustBuffer(); }} finally {{ writer.close(); }}"
            )
        }
        ReturnPlan::DirectVecViaReturnSlot { element } => {
            let buffer = callback_vector_buffer(value, element, bridge, context)?;
            format!("{buffer}\n    {out}.ref = buffer;")
        }
        _ => return unsupported("fallible callback success shape"),
    })
}

fn callback_vector_buffer(
    value: &str,
    element: &boltffi_binding::DirectVectorElementType,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    let (native, assign) = match element {
        boltffi_binding::DirectVectorElementType::Primitive(primitive) => (
            call::primitive_native_type(primitive.primitive())?.to_owned(),
            "(raw + i).value = items[i];".to_owned(),
        ),
        boltffi_binding::DirectVectorElementType::Record(id) => {
            let c_record = bridge
                .source_direct_record(*id)
                .ok_or(missing("callback vector C record"))?;
            let native = ffi::record_name(c_record);
            let record = context
                .record(*id)
                .ok_or(missing("callback vector record"))?;
            let boltffi_binding::RecordDecl::Direct(record) = record else {
                return unsupported("encoded callback direct vector record");
            };
            let assignments = record
                .fields()
                .iter()
                .zip(c_record.fields())
                .map(|(field, c_field)| {
                    format!(
                        "target.{} = item.{};",
                        c_field.name(),
                        name_style::field(field.key())
                    )
                })
                .collect::<Vec<_>>()
                .join(" ");
            (
                native,
                format!("final item = items[i]; final target = (raw + i).ref; {assignments}"),
            )
        }
        _ => return unsupported("unknown callback vector return"),
    };
    Ok(format!(
        "final items = {value};\nfinal buffer = _f$boltffi_buf_with_len(items.length * $$ffi.sizeOf<{native}>());\nfinal raw = buffer.ptr.cast<{native}>();\nfor (var i = 0; i < items.length; i++) {{ {assign} }}"
    ))
}

fn callback_async_return(
    callable: &ImportedCallable<Native>,
    slot: &crate::bridge::c::CallbackSlot,
    call_expr: &str,
    context: &RenderContext<Native>,
) -> Result<String> {
    let completion = slot
        .parameter_groups()
        .iter()
        .find_map(|group| match group {
            crate::bridge::c::ParameterGroup::CallbackCompletion(value) => Some(value),
            _ => None,
        })
        .ok_or(Error::BrokenBridgeContract {
            bridge: DartHost::TARGET,
            invariant: "missing Dart callback completion",
        })?;
    let callback = slot.parameter(completion.callback()).name();
    let callback_context = slot.parameter(completion.context()).name();
    let success_writes = callback_success_write(callable.returns().plan(), context)?;
    let writes = match callable.error().channel() {
        ErrorChannel::None => format!(
            "{}\nfinal completionCode = 0;",
            success_writes.replace("value", "result")
        ),
        ErrorChannel::Encoded { codec, .. } => {
            let error_writes = codec
                .render_with(&mut Writer::new("writer", "value", context))
                .into_iter()
                .collect::<Result<Vec<_>>>()?
                .join(" ");
            format!(
                "late final int completionCode;\nswitch (result) {{\n  case BoltFFIResult$Ok(:final value):\n    {success_writes}\n    completionCode = 0;\n  case BoltFFIResult$Err(:final value):\n    {error_writes}\n    completionCode = 100;\n}}"
            )
        }
        _ => return unsupported("async callback error encoding"),
    };
    Ok(format!(
        "final completion = {callback}.asFunction<void Function($$ffi.Pointer<$$ffi.Void>, _$$FFIStatus, _$$FFIBuf)>();\n    try {{\n      final result = await {call_expr};\n      final writer = _$$WireWriter();\n      try {{\n        {writes}\n        final buffer = writer.toRustBuffer();\n        completion({callback_context}, $$ffi.Struct.create<_$$FFIStatus>()..code = completionCode, buffer);\n      }} finally {{ writer.close(); }}\n    }} catch (_) {{\n      completion({callback_context}, $$ffi.Struct.create<_$$FFIStatus>()..code = 100, $$ffi.Struct.create<_$$FFIBuf>()..ptr = $$ffi.nullptr..len = 0..cap = 0..align = 1);\n    }}"
    ))
}

fn callback_success_write(
    plan: &ReturnPlan<Native, boltffi_binding::IntoRust>,
    context: &RenderContext<Native>,
) -> Result<String> {
    Ok(match plan {
        ReturnPlan::Void => String::new(),
        ReturnPlan::DirectViaReturnSlot { ty } | ReturnPlan::DirectViaOutPointer { ty } => match ty
        {
            DirectValueType::Primitive(value) => {
                format!("writer.write{}(value);", primitive::wire_suffix(*value)?)
            }
            DirectValueType::Enum(_) => "writer.writeI32(value.value);".into(),
            DirectValueType::Record(_) => "value._encode(writer);".into(),
            _ => return unsupported("callback direct success encoding"),
        },
        ReturnPlan::EncodedViaReturnSlot { codec, .. }
        | ReturnPlan::EncodedViaOutPointer { codec, .. } => codec
            .render_with(&mut Writer::new("writer", "value", context))
            .into_iter()
            .collect::<Result<Vec<_>>>()?
            .join(" "),
        ReturnPlan::ScalarOptionViaReturnSlot { primitive } => {
            let suffix = primitive::wire_suffix(*primitive)?;
            format!("writer.writeOptional(value, (value, writer) => writer.write{suffix}(value));")
        }
        ReturnPlan::DirectVecViaReturnSlot { element } => match element {
            boltffi_binding::DirectVectorElementType::Primitive(primitive) => {
                let suffix = primitive::wire_suffix(primitive.primitive())?;
                format!("writer.writeList(value, (value, writer) => writer.write{suffix}(value));")
            }
            boltffi_binding::DirectVectorElementType::Record(_) => {
                "writer.writeList(value, (value, writer) => value._encode(writer));".into()
            }
            _ => return unsupported("callback vector success encoding"),
        },
        ReturnPlan::HandleViaReturnSlot {
            target: HandleTarget::Class(_),
            presence,
            ..
        }
        | ReturnPlan::HandleViaOutPointer {
            target: HandleTarget::Class(_),
            presence,
            ..
        } => {
            if *presence == HandlePresence::Nullable {
                "writer.writeU64(value?._rawHandle ?? 0);".into()
            } else {
                "writer.writeU64(value._rawHandle);".into()
            }
        }
        _ => return unsupported("callback success encoding"),
    })
}

fn exported_parameters(
    callable: &boltffi_binding::ExportedCallable<Native>,
    context: &RenderContext<Native>,
) -> Result<String> {
    callable
        .params()
        .iter()
        .map(|param| {
            let ty = match param.payload() {
                boltffi_binding::IncomingParam::Value(plan) => call::parameter_type(plan, context)?,
                boltffi_binding::IncomingParam::Closure(closure) => {
                    super::closure::api_type(closure, context)?
                }
            };
            Ok(format!("{ty} {}", name_style::lower_camel(param.name())))
        })
        .collect::<Result<Vec<_>>>()
        .map(|params| params.join(", "))
}

fn imported_parameters(
    callable: &ImportedCallable<Native>,
    context: &RenderContext<Native>,
) -> Result<String> {
    callable
        .params()
        .iter()
        .map(|param| {
            let OutgoingParam::Value(plan) = param.payload() else {
                return unsupported("callback closure parameter");
            };
            Ok(format!(
                "{} {}",
                call::outgoing_parameter_type(plan, context)?,
                name_style::lower_camel(param.name())
            ))
        })
        .collect::<Result<Vec<_>>>()
        .map(|params| params.join(", "))
}

fn sync_exported_return(
    plan: &ReturnPlan<Native, boltffi_binding::OutOfRust>,
    error: ErrorChannel<Native, boltffi_binding::OutOfRust>,
    invocation: &str,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    if !matches!(error, ErrorChannel::None) {
        return unsupported("fallible synchronous exported call");
    }
    Ok(match plan {
        ReturnPlan::Void => format!("{invocation};"),
        ReturnPlan::DirectViaReturnSlot { ty } => match ty {
            DirectValueType::Primitive(_) => format!("return {invocation};"),
            DirectValueType::Enum(_) => format!(
                "return {}._fromValue({invocation});",
                call::direct_type(ty, context)?
            ),
            DirectValueType::Record(_) => format!(
                "return {}._fromStruct({invocation});",
                call::direct_type(ty, context)?
            ),
            _ => return unsupported("synchronous direct exported return"),
        },
        ReturnPlan::EncodedViaReturnSlot { codec, .. } => {
            let decode = codec.render_with(&mut Reader::new("reader", context))?;
            format!(
                "final buffer = {invocation};\ntry {{\n  final reader = _$$WireReader(buffer.ptr, buffer.len);\n  return {decode};\n}} finally {{\n  if (buffer.ptr != $$ffi.nullptr) _f$boltffi_free_buf(buffer);\n}}"
            )
        }
        ReturnPlan::HandleViaReturnSlot {
            target: HandleTarget::Class(id),
            presence,
            ..
        } => {
            let class = context
                .class(*id)
                .map(|decl| name_style::upper_camel(decl.name()))
                .ok_or(Error::BrokenBridgeContract {
                    bridge: DartHost::TARGET,
                    invariant: "missing synchronous class return",
                })?;
            if *presence == HandlePresence::Nullable {
                format!(
                    "final handle = {invocation};\nreturn handle == 0 ? null : {class}._(handle);"
                )
            } else {
                format!("return {class}._({invocation});")
            }
        }
        ReturnPlan::HandleViaReturnSlot {
            target: HandleTarget::Callback(id),
            presence,
            ..
        } => {
            let callback = context.callback(*id).ok_or(Error::BrokenBridgeContract {
                bridge: DartHost::TARGET,
                invariant: "missing synchronous callback return",
            })?;
            let name = name_style::upper_camel(callback.name());
            format!(
                "return {};",
                super::foreign::wrap_expression(&name, *presence, &format!("({invocation})"))
            )
        }
        ReturnPlan::ScalarOptionViaReturnSlot { primitive } => {
            let suffix = primitive::wire_suffix(*primitive)?;
            format!(
                "final buffer = {invocation};\ntry {{\n  final reader = _$$WireReader(buffer.ptr, buffer.len);\n  return reader.readOptional((reader) => reader.read{suffix}());\n}} finally {{\n  if (buffer.ptr != $$ffi.nullptr) _f$boltffi_free_buf(buffer);\n}}"
            )
        }
        ReturnPlan::DirectVecViaReturnSlot { element } => {
            direct_vector_decode("buffer", element, invocation, bridge, context)?
        }
        ReturnPlan::ClosureViaOutPointer(_) => {
            // Handled specially in sync_exported_call (needs out-pointer setup).
            return unsupported("closure return must use sync_exported_call path");
        }
        _ => return unsupported("synchronous exported return"),
    })
}

fn sync_exported_call(
    callable: &boltffi_binding::ExportedCallable<Native>,
    function: &crate::bridge::c::Function,
    mut args: Vec<String>,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<(String, Vec<(crate::core::HelperId, String)>)> {
    match callable.error().channel() {
        ErrorChannel::None => {
            if let ReturnPlan::ClosureViaOutPointer(closure) = callable.returns().plan() {
                return super::closure::returned_call(function, args, closure, bridge, context);
            }
            let invocation = format!("_f${}({})", function.name(), args.join(", "));
            Ok((
                sync_exported_return(
                    callable.returns().plan(),
                    ErrorChannel::None,
                    &invocation,
                    bridge,
                    context,
                )?,
                Vec::new(),
            ))
        }
        ErrorChannel::Encoded { ty, codec, .. } => {
            let success_index = function
                .parameter_groups()
                .iter()
                .find_map(|group| match group {
                    crate::bridge::c::ParameterGroup::SuccessOut(index) => Some(*index),
                    _ => None,
                });
            let (success_setup, success_cleanup, success_value) = if let Some(index) = success_index
            {
                let crate::bridge::c::Type::MutPointer(inner) = function.parameter(index).ty()
                else {
                    return Err(Error::BrokenBridgeContract {
                        bridge: DartHost::TARGET,
                        invariant: "fallible Dart call success output is not a pointer",
                    });
                };
                args.push("success".to_owned());
                (
                    format!(
                        "final success = $$extffi.calloc<{}>();\n",
                        ffi::native_type(inner)
                    ),
                    "\n  $$extffi.calloc.free(success);",
                    sync_out_pointer_success(callable.returns().plan(), bridge, context)?,
                )
            } else if matches!(callable.returns().plan(), ReturnPlan::Void) {
                (String::new(), "", "return;".to_owned())
            } else {
                return Err(Error::BrokenBridgeContract {
                    bridge: DartHost::TARGET,
                    invariant: "fallible Dart call has no success output",
                });
            };
            let error_decode = codec.render_with(&mut Reader::new("errorReader", context))?;
            let thrown = if matches!(ty, TypeRef::String) {
                format!("_$$FFIException(-1, {error_decode})")
            } else {
                error_decode
            };
            Ok((
                format!(
                    "{success_setup}try {{\n  final error = _f${}({});\n  if (error.ptr != $$ffi.nullptr) {{\n    try {{\n      final errorReader = _$$WireReader(error.ptr, error.len);\n      throw {thrown};\n    }} finally {{ _f$boltffi_free_buf(error); }}\n  }}\n  {success_value}\n}} finally {{{success_cleanup}\n}}",
                    function.name(),
                    args.join(", ")
                ),
                Vec::new(),
            ))
        }
        ErrorChannel::Status => unsupported("Dart synchronous status error"),
        _ => unsupported("unknown Dart synchronous error channel"),
    }
}

fn sync_out_pointer_success(
    plan: &ReturnPlan<Native, boltffi_binding::OutOfRust>,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    Ok(match plan {
        ReturnPlan::DirectViaOutPointer { ty } => match ty {
            DirectValueType::Primitive(_) => "return success.value;".to_owned(),
            DirectValueType::Enum(_) => format!(
                "return {}._fromValue(success.value);",
                call::direct_type(ty, context)?
            ),
            DirectValueType::Record(_) => format!(
                "return {}._fromStruct(success.ref);",
                call::direct_type(ty, context)?
            ),
            _ => return unsupported("Dart fallible direct success"),
        },
        ReturnPlan::EncodedViaOutPointer { codec, .. } => {
            let decode = codec.render_with(&mut Reader::new("reader", context))?;
            format!(
                "final buffer = success.ref;\n  try {{\n    final reader = _$$WireReader(buffer.ptr, buffer.len);\n    return {decode};\n  }} finally {{\n    if (buffer.ptr != $$ffi.nullptr) _f$boltffi_free_buf(buffer);\n  }}"
            )
        }
        ReturnPlan::HandleViaOutPointer {
            target: HandleTarget::Class(id),
            presence,
            ..
        } => {
            let class = context
                .class(*id)
                .map(|decl| name_style::upper_camel(decl.name()))
                .ok_or(missing("fallible class return"))?;
            if *presence == HandlePresence::Nullable {
                format!("return success.value == 0 ? null : {class}._(success.value);")
            } else {
                format!("return {class}._(success.value);")
            }
        }
        ReturnPlan::ScalarOptionViaReturnSlot { primitive } => {
            let suffix = primitive::wire_suffix(*primitive)?;
            format!(
                "final buffer = success.ref;\n  try {{\n    final reader = _$$WireReader(buffer.ptr, buffer.len);\n    return reader.readOptional((reader) => reader.read{suffix}());\n  }} finally {{\n    if (buffer.ptr != $$ffi.nullptr) _f$boltffi_free_buf(buffer);\n  }}"
            )
        }
        ReturnPlan::DirectVecViaReturnSlot { element } => {
            direct_vector_decode("buffer", element, "success.ref", bridge, context)?
        }
        _ => return unsupported("Dart fallible success return"),
    })
}

fn async_exported_return(
    callable: &boltffi_binding::ExportedCallable<Native>,
    protocol: &boltffi_binding::native::AsyncProtocol,
    start: String,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    let boltffi_binding::native::AsyncProtocol::PollHandle {
        poll,
        complete,
        cancel,
        free,
        ..
    } = protocol
    else {
        return unsupported("non-poll Dart async protocol");
    };
    let poll = call::c_function(poll, bridge)?;
    let complete = call::c_function(complete, bridge)?;
    let cancel = call::c_function(cancel, bridge)?;
    let free = call::c_function(free, bridge)?;
    let success_index = complete
        .parameter_groups()
        .iter()
        .find_map(|group| match group {
            crate::bridge::c::ParameterGroup::SuccessOut(index) => Some(*index),
            _ => None,
        });
    let success_setup = success_index
        .map(|index| {
            let crate::bridge::c::Type::MutPointer(inner) = complete.parameter(index).ty() else {
                return Err(Error::BrokenBridgeContract {
                    bridge: DartHost::TARGET,
                    invariant: "Dart async success output is not a pointer",
                });
            };
            Ok(format!(
                "final success = $$extffi.calloc<{}>();",
                ffi::native_type(inner)
            ))
        })
        .transpose()?
        .unwrap_or_default();
    let complete_args = complete
        .parameter_groups()
        .iter()
        .map(|group| match group {
            crate::bridge::c::ParameterGroup::Value(_) => Ok("future".to_owned()),
            crate::bridge::c::ParameterGroup::CompletionStatusOut(_) => Ok("status".to_owned()),
            crate::bridge::c::ParameterGroup::SuccessOut(_) => Ok("success".to_owned()),
            _ => unsupported("Dart async completion parameter group"),
        })
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    let call_complete = format!("_f${}({complete_args})", complete.name());
    let returns_buffer_by_value = matches!(complete.returns(), crate::bridge::c::Type::Buffer)
        && matches!(callable.error().channel(), ErrorChannel::None);
    let error = match callable.error().channel() {
        ErrorChannel::None if returns_buffer_by_value => {
            format!("final result = {call_complete};")
        }
        ErrorChannel::None => format!("{call_complete};"),
        ErrorChannel::Encoded { ty, codec, .. } => {
            let decode = codec.render_with(&mut Reader::new("errorReader", context))?;
            let thrown = match ty {
                TypeRef::String => format!("_$$FFIException(-1, {decode})"),
                _ => decode,
            };
            format!(
                "final error = {call_complete};\nif (error.ptr != $$ffi.nullptr) {{\n  try {{\n    final errorReader = _$$WireReader(error.ptr, error.len);\n    throw {thrown};\n  }} finally {{ _f$boltffi_free_buf(error); }}\n}}"
            )
        }
        ErrorChannel::Status => return unsupported("Dart async status error"),
        _ => return unsupported("unknown Dart async error"),
    };
    let success = async_success(
        callable.returns().plan(),
        success_index.is_some(),
        returns_buffer_by_value,
        bridge,
        context,
    )?;
    let success_cleanup = if success_index.is_some() {
        "\n    $$extffi.calloc.free(success);"
    } else {
        ""
    };
    let complete_body = format!(
        "final status = $$extffi.calloc<_$$FFIStatus>();\n{success_setup}\ntry {{\n  {error}\n  _$$throwIfStatus(status.ref);\n  {success}\n}} finally {{\n  $$extffi.calloc.free(status);{success_cleanup}\n}}"
    );
    let ty = call::return_type(callable.returns().plan(), context)?;
    Ok(format!(
        "return _$$BoltFFIAsync.create<{ty}>(\n  createFuture: () => {start},\n  pollFuture: _f${},\n  completeFuture: (future) {{\n    {}\n  }},\n  cancelFuture: _f${},\n  freeFuture: _f${},\n);",
        poll.name(),
        indent(&complete_body, 4),
        cancel.name(),
        free.name()
    ))
}

fn async_success(
    plan: &ReturnPlan<Native, boltffi_binding::OutOfRust>,
    out: bool,
    buffer_by_value: bool,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    Ok(match plan {
        ReturnPlan::Void => "return;".into(),
        ReturnPlan::DirectViaOutPointer { ty } => match ty {
            DirectValueType::Primitive(boltffi_binding::Primitive::Bool) => {
                "return success.value;".into()
            }
            DirectValueType::Enum(_) => format!(
                "return {}._fromValue(success.value);",
                call::direct_type(ty, context)?
            ),
            DirectValueType::Record(_) => format!(
                "return {}._fromStruct(success.ref);",
                call::direct_type(ty, context)?
            ),
            _ => "return success.value;".into(),
        },
        ReturnPlan::DirectViaReturnSlot { ty } if !out => match ty {
            DirectValueType::Enum(_) => format!(
                "return {}._fromValue(result);",
                call::direct_type(ty, context)?
            ),
            _ => "return result;".into(),
        },
        ReturnPlan::EncodedViaOutPointer { codec, .. } => {
            let decode = codec.render_with(&mut Reader::new("reader", context))?;
            format!(
                "final buffer = success.ref;\ntry {{\n  final reader = _$$WireReader(buffer.ptr, buffer.len);\n  return {decode};\n}} finally {{ if (buffer.ptr != $$ffi.nullptr) _f$boltffi_free_buf(buffer); }}"
            )
        }
        ReturnPlan::EncodedViaReturnSlot { codec, .. } if buffer_by_value => {
            let decode = codec.render_with(&mut Reader::new("reader", context))?;
            format!(
                "try {{\n  final reader = _$$WireReader(result.ptr, result.len);\n  return {decode};\n}} finally {{ if (result.ptr != $$ffi.nullptr) _f$boltffi_free_buf(result); }}"
            )
        }
        ReturnPlan::HandleViaOutPointer {
            target: HandleTarget::Class(id),
            presence,
            ..
        } => {
            let name = context
                .class(*id)
                .map(|decl| name_style::upper_camel(decl.name()))
                .ok_or(Error::BrokenBridgeContract {
                    bridge: DartHost::TARGET,
                    invariant: "missing async class return",
                })?;
            if *presence == HandlePresence::Nullable {
                format!("return success.value == 0 ? null : {name}._(success.value);")
            } else {
                format!("return {name}._(success.value);")
            }
        }
        ReturnPlan::ScalarOptionViaReturnSlot { primitive } if out => {
            let suffix = primitive::wire_suffix(*primitive)?;
            format!(
                "final buffer = success.ref;\ntry {{\n  final reader = _$$WireReader(buffer.ptr, buffer.len);\n  return reader.readOptional((reader) => reader.read{suffix}());\n}} finally {{ if (buffer.ptr != $$ffi.nullptr) _f$boltffi_free_buf(buffer); }}"
            )
        }
        ReturnPlan::ScalarOptionViaReturnSlot { primitive } if buffer_by_value => {
            let suffix = primitive::wire_suffix(*primitive)?;
            format!(
                "try {{\n  final reader = _$$WireReader(result.ptr, result.len);\n  return reader.readOptional((reader) => reader.read{suffix}());\n}} finally {{ if (result.ptr != $$ffi.nullptr) _f$boltffi_free_buf(result); }}"
            )
        }
        ReturnPlan::DirectVecViaReturnSlot { element } if out => {
            direct_vector_decode("buffer", element, "success.ref", bridge, context)?
        }
        ReturnPlan::DirectVecViaReturnSlot { element } if buffer_by_value => {
            direct_vector_decode("buffer", element, "result", bridge, context)?
        }
        _ => return unsupported("Dart async success shape"),
    })
}

pub(super) fn direct_vector_decode_public(
    buffer: &str,
    element: &boltffi_binding::DirectVectorElementType,
    value: &str,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    direct_vector_decode(buffer, element, value, bridge, context)
}

fn direct_vector_decode(
    buffer: &str,
    element: &boltffi_binding::DirectVectorElementType,
    value: &str,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    let (native, decode) = match element {
        boltffi_binding::DirectVectorElementType::Primitive(primitive) => {
            let native = call::primitive_native_type(primitive.primitive())?;
            (native.to_owned(), "(raw + i).value".to_owned())
        }
        boltffi_binding::DirectVectorElementType::Record(id) => {
            let c_record = bridge
                .source_direct_record(*id)
                .ok_or(Error::BrokenBridgeContract {
                    bridge: DartHost::TARGET,
                    invariant: "missing returned direct vector record",
                })?;
            let native = ffi::record_name(c_record);
            let public = context
                .record(*id)
                .map(|record| name_style::upper_camel(record.name()))
                .ok_or(missing("returned direct vector record"))?;
            (native, format!("{public}._fromStruct((raw + i).ref)"))
        }
        _ => return unsupported("unknown returned direct vector"),
    };
    Ok(format!(
        "final {buffer} = {value};\ntry {{\n  final count = {buffer}.len ~/ $$ffi.sizeOf<{native}>();\n  final raw = {buffer}.ptr.cast<{native}>();\n  return List.generate(count, (i) => {decode}, growable: false);\n}} finally {{\n  if ({buffer}.ptr != $$ffi.nullptr) _f$boltffi_free_buf({buffer});\n}}"
    ))
}

fn wrap_call(setup: Vec<String>, cleanup: Vec<String>, body: String) -> String {
    if setup.is_empty() {
        return body;
    }
    format!(
        "{}\ntry {{\n  {}\n}} finally {{\n  {}\n}}",
        setup.join("\n"),
        body.replace('\n', "\n  "),
        cleanup.join("\n  ")
    )
}

fn callback_exceptional_return(ty: &crate::bridge::c::Type) -> String {
    match ty {
        crate::bridge::c::Type::Void
        | crate::bridge::c::Type::Buffer
        | crate::bridge::c::Type::String
        | crate::bridge::c::Type::Span
        | crate::bridge::c::Type::Status
        | crate::bridge::c::Type::CallbackHandle(_)
        | crate::bridge::c::Type::Named(_)
        | crate::bridge::c::Type::DirectRecord(_)
        | crate::bridge::c::Type::ConstPointer(_)
        | crate::bridge::c::Type::MutPointer(_)
        | crate::bridge::c::Type::FunctionPointer { .. } => String::new(),
        crate::bridge::c::Type::Bool => ", false".into(),
        crate::bridge::c::Type::Float32 | crate::bridge::c::Type::Float64 => ", 0.0".into(),
        _ => ", 0".into(),
    }
}

fn indent(value: &str, spaces: usize) -> String {
    value.replace('\n', &format!("\n{}", " ".repeat(spaces)))
}

fn data_enum(
    value: &boltffi_binding::DataEnumDecl<Native>,
    context: &RenderContext<Native>,
) -> Result<Emitted> {
    let name = name_style::upper_camel(value.name());
    let mut classes = Vec::new();
    let mut cases = Vec::new();
    let mut factories = Vec::new();
    for variant in value.variants() {
        let variant_name = format!("{name}${}", name_style::upper_camel(variant.name()));
        let factory_name = name_style::lower_camel(variant.name());
        let fields = match variant.payload() {
            DataVariantPayload::Unit => &[][..],
            DataVariantPayload::Tuple(fields) | DataVariantPayload::Struct(fields) => {
                fields.as_slice()
            }
            _ => return unsupported("unknown data enum payload"),
        };
        let declarations = fields
            .iter()
            .map(|field| {
                Ok(format!(
                    "  final {} {};",
                    dart_type(field.ty(), context)?,
                    name_style::field(field.key())
                ))
            })
            .collect::<Result<Vec<_>>>()?
            .join("\n");
        let reads = fields
            .iter()
            .map(|field| {
                field
                    .read()
                    .render_with(&mut Reader::new("reader", context))
            })
            .collect::<Result<Vec<_>>>()?;
        let writes = fields
            .iter()
            .flat_map(|field| {
                field
                    .write()
                    .render_with(&mut Writer::new("writer", "", context))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(|line| format!("    {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let (factory_params, ctor_params, decode_args) = match variant.payload() {
            DataVariantPayload::Unit => (String::new(), String::new(), String::new()),
            DataVariantPayload::Tuple(_) => (
                fields
                    .iter()
                    .map(|field| {
                        Ok(format!(
                            "{} {}",
                            dart_type(field.ty(), context)?,
                            name_style::field(field.key())
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?
                    .join(", "),
                fields
                    .iter()
                    .map(|field| format!("this.{}", name_style::field(field.key())))
                    .collect::<Vec<_>>()
                    .join(", "),
                reads.join(", "),
            ),
            DataVariantPayload::Struct(_) => (
                format!(
                    "{{{}}}",
                    fields
                        .iter()
                        .map(|field| Ok(format!(
                            "required {} {}",
                            dart_type(field.ty(), context)?,
                            name_style::field(field.key())
                        )))
                        .collect::<Result<Vec<_>>>()?
                        .join(", ")
                ),
                format!(
                    "{{{}}}",
                    fields
                        .iter()
                        .map(|field| format!("required this.{}", name_style::field(field.key())))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                fields
                    .iter()
                    .zip(&reads)
                    .map(|(field, read)| format!("{}: {read}", name_style::field(field.key())))
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            _ => return unsupported("unknown data enum payload"),
        };
        factories.push(format!(
            "  const factory {name}.{factory_name}({factory_params}) = {variant_name};"
        ));
        let ctor = format!("const {variant_name}({ctor_params})");
        classes.push(format!("final class {variant_name} extends {name} {{\n{declarations}\n  {ctor};\n  @override\n  void _encode(_$$WireWriter writer) {{\n    writer.writeU32({});\n{writes}\n  }}\n}}", variant.tag().get()));
        cases.push(format!(
            "      {} => {variant_name}({decode_args}),",
            variant.tag().get()
        ));
    }
    Ok(Emitted::primary(format!(
        "sealed class {name}{} {{\n  const {name}();\n{}\n  factory {name}._decode(_$$WireReader reader) {{\n    return switch (reader.readU32()) {{\n{}\n      final tag => throw StateError('Unknown {name} tag: $tag'),\n    }};\n  }}\n  void _encode(_$$WireWriter writer);\n}}\n\n{}\n\n",
        if value.is_error_payload() {
            " implements Exception"
        } else {
            ""
        },
        factories.join("\n"),
        cases.join("\n"),
        classes.join("\n\n")
    )))
}

fn missing(kind: &'static str) -> Error {
    Error::BrokenBridgeContract {
        bridge: DartHost::TARGET,
        invariant: kind,
    }
}
fn unsupported<T>(shape: &'static str) -> Result<T> {
    Err(Error::UnsupportedTarget {
        target: DartHost::TARGET,
        shape,
    })
}

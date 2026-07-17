//! Foreign callback proxies: Rust-owned callback handles wrapped for Dart use.

use boltffi_binding::{
    BuiltinType, CallbackDecl, DirectValueType, DirectVectorElementType, ErrorChannel,
    HandlePresence, HandleTarget, ImportedCallable, ImportedMethodDecl, Native, OutgoingParam,
    ParamPlan, ReturnPlan, TypeRef, VTableSlot,
};

use crate::{
    bridge::c::{self, CBridgeContract, CallbackSlot},
    core::{Error, RenderContext, Result},
};

use super::{DartHost, call, ffi, name_style, primitive, render};

pub fn wrap_expression(callback_name: &str, presence: HandlePresence, value: &str) -> String {
    match presence {
        HandlePresence::Nullable => {
            format!(
                "(() {{ final handle = {value}; return handle.handle == 0 ? null : _F${callback_name}.wrap(handle); }})()"
            )
        }
        _ => format!("_F${callback_name}.wrap({value})"),
    }
}

pub fn proxy_class(
    decl: &CallbackDecl<Native>,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    let protocol = bridge
        .source_callback(decl.id())
        .ok_or(Error::BrokenBridgeContract {
            bridge: DartHost::TARGET,
            invariant: "missing Dart foreign callback C protocol",
        })?;
    let name = name_style::upper_camel(decl.name());
    let vtable_name = ffi::record_name(protocol.vtable());
    let methods = decl
        .protocol()
        .vtable()
        .methods()
        .iter()
        .zip(protocol.methods())
        .filter(|(method, _)| !method.callable().execution().uses_async_execution())
        .map(|(method, slot)| proxy_method(&name, &vtable_name, method, slot, bridge, context))
        .collect::<Result<Vec<_>>>()?
        .join("\n\n");

    Ok(format!(
        "final class _F${name} implements {name} {{\n  static final _finalizer = Finalizer<_$$BoltFFICallbackHandle>((handle) {{\n    if (handle.handle == 0 || handle.vtable == $$ffi.nullptr) return;\n    final vtable = handle.vtable.cast<{vtable_name}>();\n    vtable.ref.free.asFunction<void Function(int)>()(handle.handle);\n  }});\n  _$$BoltFFICallbackHandle _handle;\n  bool _closed = false;\n\n  _F${name}._(this._handle) {{\n    _finalizer.attach(this, _handle, detach: this);\n  }}\n\n  static {name} wrap(_$$BoltFFICallbackHandle handle) {{\n    if (handle.handle == 0 || handle.vtable == $$ffi.nullptr) {{\n      throw StateError('null BoltFFI callback handle');\n    }}\n    return _F${name}._(handle);\n  }}\n\n  void dispose() {{\n    if (_closed) return;\n    _closed = true;\n    _finalizer.detach(this);\n    final vtable = _handle.vtable.cast<{vtable_name}>();\n    final free = vtable.ref.free.asFunction<void Function(int)>();\n    free(_handle.handle);\n    _handle = _$$nullCallbackHandle();\n  }}\n\n{methods}\n}}\n\n"
    ))
}

fn proxy_method(
    _callback_name: &str,
    vtable_name: &str,
    method: &ImportedMethodDecl<Native, VTableSlot>,
    slot: &CallbackSlot,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    let callable = method.callable();
    if callable.execution().uses_async_execution() {
        return unsupported("async foreign callback proxy method");
    }
    let api_params = method
        .callable()
        .params()
        .iter()
        .map(|param| {
            let OutgoingParam::Value(plan) = param.payload() else {
                return unsupported("foreign callback proxy closure parameter");
            };
            Ok(format!(
                "{} {}",
                call::outgoing_parameter_type(plan, context)?,
                name_style::lower_camel(param.name())
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    let returns = call::callback_api_return(callable, context)?;
    let (setup, cleanup, call_args) = marshal_proxy_args(callable, slot, bridge, context)?;
    let native_call = format!(
        "final vtable = _handle.vtable.cast<{vtable_name}>();\nfinal invoke = vtable.ref.{}.asFunction<{} Function({})>();\nfinal raw = invoke({});",
        slot.name().as_str(),
        ffi::dart_type(slot.returns()),
        slot.parameters()
            .iter()
            .map(|parameter| ffi::dart_type(parameter.ty()))
            .collect::<Vec<_>>()
            .join(", "),
        call_args.join(", ")
    );
    let decode = decode_proxy_return(callable, slot, "raw", bridge, context)?;
    let mut body = vec!["if (_closed) { throw StateError('released foreign callback'); }".into()];
    body.extend(setup);
    body.push(native_call);
    body.push(decode);
    let body = if cleanup.is_empty() {
        body.join("\n")
    } else {
        format!(
            "{}\ntry {{\n  {}\n}} finally {{\n  {}\n}}",
            body[..body.len().saturating_sub(1)].join("\n"),
            body.last()
                .cloned()
                .unwrap_or_default()
                .replace('\n', "\n  "),
            cleanup.join("\n  ")
        )
    };
    Ok(format!(
        "  @override\n  {returns} {}({api_params}) {{\n    {}\n  }}",
        name_style::lower_camel(method.name()),
        indent(&body, 4)
    ))
}

fn marshal_proxy_args(
    callable: &ImportedCallable<Native>,
    slot: &CallbackSlot,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let mut setup = Vec::new();
    let mut cleanup = Vec::new();
    let mut args = vec!["_handle.handle".to_owned()];

    // Slot parameters: handle + return outs + source args.
    // Collect success-out first for fallible calls.
    for group in slot.return_parameter_groups() {
        match group {
            c::ParameterGroup::SuccessOut(index) => {
                let param = slot.parameter(*index);
                let local = format!("_$out{}", param.name());
                let ty = match param.ty() {
                    c::Type::MutPointer(inner) => ffi::native_type(inner),
                    _ => {
                        return Err(Error::BrokenBridgeContract {
                            bridge: DartHost::TARGET,
                            invariant: "foreign callback success out is not a pointer",
                        });
                    }
                };
                setup.push(format!("final {local} = $$extffi.calloc<{ty}>();"));
                cleanup.push(format!("$$extffi.calloc.free({local});"));
                args.push(local);
            }
            _ => {}
        }
    }

    for (param, group) in callable.params().iter().zip(slot.source_parameter_groups()) {
        let OutgoingParam::Value(plan) = param.payload() else {
            return unsupported("foreign callback proxy nested closure");
        };
        let name = name_style::lower_camel(param.name());
        match (plan, group) {
            (ParamPlan::Direct { ty, .. }, c::ParameterGroup::Value(_)) => match ty {
                DirectValueType::Primitive(boltffi_binding::Primitive::Bool) => {
                    args.push(format!("{name} ? true : false"))
                }
                DirectValueType::Enum(_) => args.push(format!("{name}.value")),
                DirectValueType::Record(_) => args.push(format!("{name}._toStruct()")),
                _ => args.push(name),
            },
            (ParamPlan::Encoded { ty, .. }, c::ParameterGroup::ByteSlice(_)) => {
                let writer = format!("_$writer{}", name_style::upper_camel(param.name()));
                let writes = encode_type_ref(ty, &writer, &name, context)?;
                setup.push(format!("final {writer} = _$$WireWriter();\n    {writes}"));
                cleanup.push(format!("{writer}.close();"));
                args.extend([format!("{writer}.ptr"), format!("{writer}.len")]);
            }
            (ParamPlan::ScalarOption { primitive }, c::ParameterGroup::ByteSlice(_)) => {
                let writer = format!("_$writer{}", name_style::upper_camel(param.name()));
                let suffix = primitive::wire_suffix(*primitive)?;
                setup.push(format!(
                    "final {writer} = _$$WireWriter();\n    {writer}.writeOptional({name}, (value, writer) => writer.write{suffix}(value));"
                ));
                cleanup.push(format!("{writer}.close();"));
                args.extend([format!("{writer}.ptr"), format!("{writer}.len")]);
            }
            (ParamPlan::DirectVec { element, .. }, c::ParameterGroup::DirectVector(_)) => {
                let local = format!("_$vector{}", name_style::upper_camel(param.name()));
                match element {
                    DirectVectorElementType::Primitive(value) => {
                        let native = call::primitive_native_type(value.primitive())?;
                        setup.push(format!(
                            "final {local} = $$extffi.calloc<{native}>({name}.length);\n    for (var i = 0; i < {name}.length; i++) {{ ({local} + i).value = {name}[i]; }}"
                        ));
                        cleanup.push(format!("$$extffi.calloc.free({local});"));
                        args.extend([local, format!("{name}.length")]);
                    }
                    DirectVectorElementType::Record(id) => {
                        let c_record = bridge.source_direct_record(*id).ok_or(
                            Error::BrokenBridgeContract {
                                bridge: DartHost::TARGET,
                                invariant: "missing foreign proxy vector record",
                            },
                        )?;
                        let c_name = ffi::record_name(c_record);
                        let record = context.record(*id).ok_or(missing("foreign proxy record"))?;
                        let boltffi_binding::RecordDecl::Direct(record) = record else {
                            return unsupported("encoded foreign proxy vector record");
                        };
                        let copies = record
                            .fields()
                            .iter()
                            .zip(c_record.fields())
                            .map(|(field, c_field)| {
                                format!(
                                    "target.{} = value.{};",
                                    c_field.name(),
                                    name_style::field(field.key())
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(" ");
                        setup.push(format!(
                            "final {local} = $$extffi.calloc<{c_name}>({name}.length);\n    for (var i = 0; i < {name}.length; i++) {{ final value = {name}[i]; final target = ({local} + i).ref; {copies} }}"
                        ));
                        cleanup.push(format!("$$extffi.calloc.free({local});"));
                        args.extend([
                            format!("{local}.cast<$$ffi.Uint8>()"),
                            format!("{name}.length * $$ffi.sizeOf<{c_name}>()"),
                        ]);
                    }
                    _ => return unsupported("foreign proxy direct vector"),
                }
            }
            (
                ParamPlan::Handle {
                    target, presence, ..
                },
                c::ParameterGroup::Value(_),
            ) => match target {
                HandleTarget::Class(_) => {
                    if *presence == HandlePresence::Nullable {
                        args.push(format!("{name}?._rawHandle ?? 0"));
                    } else {
                        args.push(format!("{name}._rawHandle"));
                    }
                }
                HandleTarget::Callback(id) => {
                    let callback = context
                        .callback(*id)
                        .ok_or(missing("foreign nested callback"))?;
                    let map = format!("_I${}HandleMap", name_style::upper_camel(callback.name()));
                    if *presence == HandlePresence::Nullable {
                        args.push(format!(
                            "{name} == null ? _$$nullCallbackHandle() : {map}.createHandle({name})"
                        ));
                    } else {
                        args.push(format!("{map}.createHandle({name})"));
                    }
                }
                _ => return unsupported("foreign proxy handle target"),
            },
            _ => return unsupported("foreign proxy argument shape"),
        }
    }
    let _ = bridge;
    Ok((setup, cleanup, args))
}

fn encode_type_ref(
    ty: &TypeRef,
    writer: &str,
    value: &str,
    context: &RenderContext<Native>,
) -> Result<String> {
    Ok(match ty {
        TypeRef::String | TypeRef::InternedString { .. } => {
            format!("{writer}.writeString({value});")
        }
        TypeRef::Bytes => {
            format!("{writer}.writeU32({value}.length); {writer}.writeTypedList({value});")
        }
        TypeRef::Builtin(BuiltinType::Duration) => {
            format!("{writer}.writeDuration({value});")
        }
        TypeRef::Builtin(BuiltinType::SystemTime) => {
            format!("{writer}.writeInstant({value});")
        }
        TypeRef::Builtin(BuiltinType::Uuid) => format!("{writer}.writeUuid({value});"),
        TypeRef::Builtin(BuiltinType::Url) => format!("{writer}.writeString({value});"),
        TypeRef::Record(_) | TypeRef::Enum(_) | TypeRef::Custom(_) => {
            format!("{value}._encode({writer});")
        }
        TypeRef::Optional(inner) => {
            let inner_write = encode_type_ref(inner, "writer", "item", context)?;
            format!("{writer}.writeOptional({value}, (item, writer) {{ {inner_write} }});")
        }
        TypeRef::Sequence(inner) => {
            let inner_write = encode_type_ref(inner, "writer", "item", context)?;
            format!("{writer}.writeList({value}, (item, writer) {{ {inner_write} }});")
        }
        _ => {
            let _ = context;
            return unsupported("foreign proxy encoded argument type");
        }
    })
}

fn decode_type_ref(ty: &TypeRef, reader: &str, context: &RenderContext<Native>) -> Result<String> {
    Ok(match ty {
        TypeRef::String | TypeRef::InternedString { .. } => format!("{reader}.readString()"),
        TypeRef::Bytes => format!("{reader}.readUint8List()"),
        TypeRef::Builtin(BuiltinType::Duration) => format!("{reader}.readDuration()"),
        TypeRef::Builtin(BuiltinType::SystemTime) => format!("{reader}.readInstant()"),
        TypeRef::Builtin(BuiltinType::Uuid) => format!("{reader}.readUuid()"),
        TypeRef::Builtin(BuiltinType::Url) => format!("{reader}.readString()"),
        TypeRef::Record(id) => {
            let name = context
                .record(*id)
                .map(|decl| name_style::upper_camel(decl.name()))
                .ok_or(missing("foreign proxy decode record"))?;
            format!("{name}._decode({reader})")
        }
        TypeRef::Enum(id) => {
            let name = context
                .enumeration(*id)
                .map(|decl| name_style::upper_camel(decl.name()))
                .ok_or(missing("foreign proxy decode enum"))?;
            format!("{name}._decode({reader})")
        }
        TypeRef::Custom(id) => {
            let name = context
                .custom_type(*id)
                .map(|decl| name_style::upper_camel(decl.name()))
                .ok_or(missing("foreign proxy decode custom"))?;
            // Custom typedefs share representation decode.
            let representation = context
                .custom_type(*id)
                .map(|decl| decl.representation().clone())
                .ok_or(missing("foreign proxy custom representation"))?;
            let _ = name;
            decode_type_ref(&representation, reader, context)?
        }
        TypeRef::Optional(inner) => {
            let inner = decode_type_ref(inner, reader, context)?;
            format!("{reader}.readOptional(({reader}) => {inner})")
        }
        TypeRef::Sequence(inner) => {
            let inner = decode_type_ref(inner, reader, context)?;
            format!("{reader}.readList(({reader}) => {inner})")
        }
        _ => return unsupported("foreign proxy encoded return type"),
    })
}

fn decode_proxy_return(
    callable: &ImportedCallable<Native>,
    slot: &CallbackSlot,
    raw: &str,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    match callable.error().channel() {
        ErrorChannel::None => decode_infallible(callable.returns().plan(), raw, bridge, context),
        ErrorChannel::Encoded { ty, .. } => {
            let success_out = slot.return_parameter_groups().iter().find_map(|group| {
                if let c::ParameterGroup::SuccessOut(index) = group {
                    Some(format!("_$out{}", slot.parameter(*index).name()))
                } else {
                    None
                }
            });
            let success_value = decode_fallible_success_value(
                callable.returns().plan(),
                success_out.as_deref(),
                bridge,
                context,
            )?;
            let error = decode_type_ref(ty, "errorReader", context)?;
            Ok(format!(
                "if ({raw}.ptr != $$ffi.nullptr) {{\n  try {{\n    final errorReader = _$$WireReader({raw}.ptr, {raw}.len);\n    return BoltFFIResult.err({error});\n  }} finally {{ _f$boltffi_free_buf({raw}); }}\n}}\nreturn BoltFFIResult.ok({success_value});"
            ))
        }
        ErrorChannel::Status => unsupported("foreign proxy status error"),
        _ => unsupported("foreign proxy error channel"),
    }
}

fn decode_infallible(
    plan: &ReturnPlan<Native, boltffi_binding::IntoRust>,
    raw: &str,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    Ok(match plan {
        ReturnPlan::Void => format!("{raw};"),
        ReturnPlan::DirectViaReturnSlot { ty } => match ty {
            DirectValueType::Primitive(_) => format!("return {raw};"),
            DirectValueType::Enum(_) => format!(
                "return {}._fromValue({raw});",
                call::direct_type(ty, context)?
            ),
            DirectValueType::Record(_) => format!(
                "return {}._fromStruct({raw});",
                call::direct_type(ty, context)?
            ),
            _ => return unsupported("foreign proxy direct return"),
        },
        ReturnPlan::EncodedViaReturnSlot { ty, .. } => {
            let decode = decode_type_ref(ty, "reader", context)?;
            format!(
                "try {{\n  final reader = _$$WireReader({raw}.ptr, {raw}.len);\n  return {decode};\n}} finally {{\n  if ({raw}.ptr != $$ffi.nullptr) _f$boltffi_free_buf({raw});\n}}"
            )
        }
        ReturnPlan::ScalarOptionViaReturnSlot { primitive } => {
            let suffix = primitive::wire_suffix(*primitive)?;
            format!(
                "try {{\n  final reader = _$$WireReader({raw}.ptr, {raw}.len);\n  return reader.readOptional((reader) => reader.read{suffix}());\n}} finally {{\n  if ({raw}.ptr != $$ffi.nullptr) _f$boltffi_free_buf({raw});\n}}"
            )
        }
        ReturnPlan::DirectVecViaReturnSlot { element } => {
            render::direct_vector_decode_public("buffer", element, raw, bridge, context)?
        }
        ReturnPlan::HandleViaReturnSlot {
            target: HandleTarget::Class(id),
            presence,
            ..
        } => {
            let class = context
                .class(*id)
                .map(|decl| name_style::upper_camel(decl.name()))
                .ok_or(missing("foreign proxy class return"))?;
            if *presence == HandlePresence::Nullable {
                format!("return {raw} == 0 ? null : {class}._({raw});")
            } else {
                format!("return {class}._({raw});")
            }
        }
        ReturnPlan::HandleViaReturnSlot {
            target: HandleTarget::Callback(id),
            presence,
            ..
        } => {
            let callback = context
                .callback(*id)
                .ok_or(missing("foreign proxy callback return"))?;
            let name = name_style::upper_camel(callback.name());
            format!("return {};", wrap_expression(&name, *presence, raw))
        }
        _ => return unsupported("foreign proxy return shape"),
    })
}

fn decode_fallible_success_value(
    plan: &ReturnPlan<Native, boltffi_binding::IntoRust>,
    out: Option<&str>,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    if matches!(plan, ReturnPlan::Void) {
        return Ok("null".into());
    }
    let out = out.ok_or(Error::BrokenBridgeContract {
        bridge: DartHost::TARGET,
        invariant: "foreign fallible success out missing",
    })?;
    Ok(match plan {
        ReturnPlan::DirectViaOutPointer { ty } => match ty {
            DirectValueType::Primitive(_) => format!("{out}.value"),
            DirectValueType::Enum(_) => format!(
                "{}._fromValue({out}.value)",
                call::direct_type(ty, context)?
            ),
            DirectValueType::Record(_) => {
                format!("{}._fromStruct({out}.ref)", call::direct_type(ty, context)?)
            }
            _ => return unsupported("foreign fallible direct success"),
        },
        ReturnPlan::EncodedViaOutPointer { ty, .. } => {
            let decode = decode_type_ref(ty, "reader", context)?;
            // Expression form is awkward for buffer frees; use a block expression.
            format!(
                "() {{ final buffer = {out}.ref; try {{ final reader = _$$WireReader(buffer.ptr, buffer.len); return {decode}; }} finally {{ if (buffer.ptr != $$ffi.nullptr) _f$boltffi_free_buf(buffer); }} }}()"
            )
        }
        ReturnPlan::ScalarOptionViaReturnSlot { primitive } => {
            let suffix = primitive::wire_suffix(*primitive)?;
            format!(
                "() {{ final buffer = {out}.ref; try {{ final reader = _$$WireReader(buffer.ptr, buffer.len); return reader.readOptional((reader) => reader.read{suffix}()); }} finally {{ if (buffer.ptr != $$ffi.nullptr) _f$boltffi_free_buf(buffer); }} }}()"
            )
        }
        ReturnPlan::DirectVecViaReturnSlot { element } => {
            // Keep as expression via IIFE around statements.
            let body = render::direct_vector_decode_public(
                "buffer",
                element,
                &format!("{out}.ref"),
                bridge,
                context,
            )?;
            format!("() {{ {body} }}()")
        }
        _ => return unsupported("foreign fallible success shape"),
    })
}

fn indent(value: &str, spaces: usize) -> String {
    value.replace('\n', &format!("\n{}", " ".repeat(spaces)))
}

fn missing(shape: &'static str) -> Error {
    Error::BrokenBridgeContract {
        bridge: DartHost::TARGET,
        invariant: shape,
    }
}

fn unsupported<T>(shape: &'static str) -> Result<T> {
    Err(Error::UnsupportedTarget {
        target: DartHost::TARGET,
        shape,
    })
}

//! Inline closures for the Dart C-ABI target.
//!
//! **Parameters** (`impl Fn` into Rust): Dart boxes the function, exposes
//! call/release trampolines via `Pointer.fromFunction`, and passes
//! `Pointer.fromAddress(handle)` as context.
//!
//! **Returns** (`impl Fn` out of Rust): Dart allocates a
//! `{invoke, context, release}` storage struct, Rust fills it through a
//! `void*` out-pointer, and Dart wraps it as a language function with a
//! finalizer that runs `release`.

use boltffi_binding::{
    ClosureParameter, ClosureReturn, DirectValueType, DirectVectorElementType, ErrorChannel,
    HandlePresence, HandleTarget, ImportedCallable, IncomingParam, Native, OutgoingParam, ParamPlan,
    ReturnPlan,
};

use crate::{
    bridge::c::{self, CBridgeContract, Function},
    core::{AuxChunk, Emitted, Error, HelperId, RenderContext, Result, TextChunk},
};

use super::{
    DartHost, call,
    codec::{Reader, Writer},
    ffi, name_style, primitive,
};

pub fn api_type(
    closure: &ClosureParameter<Native, boltffi_binding::IntoRust>,
    context: &RenderContext<Native>,
) -> Result<String> {
    if !matches!(
        closure.presence(),
        HandlePresence::Required | HandlePresence::Nullable
    ) {
        return unsupported("unknown closure presence");
    }
    let invoke = closure.invoke();
    let params = invoke
        .params()
        .iter()
        .map(|param| {
            let OutgoingParam::Value(plan) = param.payload() else {
                return unsupported("nested closure parameter");
            };
            call::outgoing_parameter_type(plan, context)
        })
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    let returns = call::callback_api_return(invoke, context)?;
    let ty = format!("{returns} Function({params})");
    Ok(match closure.presence() {
        HandlePresence::Nullable => format!("{ty}?"),
        _ => ty,
    })
}

pub fn helper_id(signature: &str) -> HelperId {
    HelperId::new(boltffi_binding::CanonicalName::single(format!(
        "closure_{signature}"
    )))
}

pub fn class_name(signature: &str) -> String {
    format!("_Cl${}", signature.replace(['<', '>', ',', ' '], "_"))
}

/// Emit the shared trampoline class for one closure signature.
pub fn render_helper(
    closure: &ClosureParameter<Native, boltffi_binding::IntoRust>,
    c_closure: &c::ClosureParameter,
    call_ty: &c::Type,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<(HelperId, String)> {
    let signature = closure.signature().as_str();
    let class = class_name(signature);
    let invoke = closure.invoke();
    let api = api_type(closure, context)?;
    let map_ty = api.trim_end_matches('?').to_owned();

    let c::Type::FunctionPointer {
        returns: call_returns,
        params: call_params,
    } = call_ty
    else {
        return Err(Error::BrokenBridgeContract {
            bridge: DartHost::TARGET,
            invariant: "closure call parameter is not a function pointer",
        });
    };

    let native_params = call_params
        .iter()
        .map(ffi::native_type)
        .collect::<Vec<_>>()
        .join(", ");
    let dart_params = call_params
        .iter()
        .enumerate()
        .map(|(index, ty)| format!("{} p{index}", ffi::dart_type(ty)))
        .collect::<Vec<_>>()
        .join(", ");
    let exceptional = exceptional_return(call_returns);
    let body = indent(&invoke_body(invoke, c_closure, bridge, context)?, 4);

    let source = format!(
        "final class {class} {{\n  static final Map<int, {map_ty}> _map = {{}};\n  static int _counter = 1;\n\n  static int insert({map_ty} value) {{\n    final handle = _counter += 2;\n    _map[handle] = value;\n    return handle;\n  }}\n\n  static void release($$ffi.Pointer<$$ffi.Void> context) {{\n    _map.remove(context.address);\n  }}\n\n  static {} call({dart_params}) {{\n{body}\n  }}\n\n  static final callPtr = $$ffi.Pointer.fromFunction<\n    {} Function({native_params})\n  >(call{exceptional});\n  static final releasePtr = $$ffi.Pointer.fromFunction<\n    $$ffi.Void Function($$ffi.Pointer<$$ffi.Void>)\n  >(release);\n}}\n\n",
        ffi::dart_type(call_returns),
        ffi::native_type(call_returns),
    );

    Ok((helper_id(signature), source))
}

pub fn marshal(
    name: &str,
    closure: &ClosureParameter<Native, boltffi_binding::IntoRust>,
) -> Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let class = class_name(closure.signature().as_str());
    let handle = format!(
        "_$handle{}",
        name.chars()
            .next()
            .map(|c| c.to_uppercase().collect::<String>())
            .unwrap_or_default()
            + &name.chars().skip(1).collect::<String>()
    );
    match closure.presence() {
        HandlePresence::Required => Ok((
            vec![format!("final {handle} = {class}.insert({name});")],
            Vec::new(),
            vec![
                format!("{class}.callPtr"),
                format!("$$ffi.Pointer<$$ffi.Void>.fromAddress({handle})"),
                format!("{class}.releasePtr"),
            ],
        )),
        HandlePresence::Nullable => Ok((
            vec![format!(
                "final {handle} = {name} == null ? 0 : {class}.insert({name});"
            )],
            Vec::new(),
            vec![
                format!("{handle} == 0 ? $$ffi.nullptr : {class}.callPtr"),
                format!(
                    "{handle} == 0 ? $$ffi.nullptr : $$ffi.Pointer<$$ffi.Void>.fromAddress({handle})"
                ),
                format!("{handle} == 0 ? $$ffi.nullptr : {class}.releasePtr"),
            ],
        )),
        _ => unsupported("unknown closure presence"),
    }
}

pub fn attach_helpers(mut emitted: Emitted, helpers: Vec<(HelperId, String)>) -> Emitted {
    for (id, text) in helpers {
        emitted = emitted.with_aux(AuxChunk::Helper {
            id,
            text: TextChunk::from(text),
        });
    }
    emitted
}

// ---------------------------------------------------------------------------
// Returned closures (Rust → Dart)
// ---------------------------------------------------------------------------

pub fn returned_api_type(
    closure: &ClosureReturn<Native, boltffi_binding::OutOfRust>,
    context: &RenderContext<Native>,
) -> Result<String> {
    let invoke = closure.invoke();
    let params = invoke
        .params()
        .iter()
        .map(|param| {
            let IncomingParam::Value(plan) = param.payload() else {
                return unsupported("nested closure in returned closure");
            };
            call::parameter_type(plan, context)
        })
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    let returns = call::return_type(invoke.returns().plan(), context)?;
    let ty = format!("{returns} Function({params})");
    Ok(match closure.presence() {
        HandlePresence::Nullable => format!("{ty}?"),
        _ => ty,
    })
}

pub fn returned_storage_name(signature: &str) -> String {
    format!("_ClRet${}", signature.replace(['<', '>', ',', ' '], "_"))
}

pub fn returned_helper(
    closure: &ClosureReturn<Native, boltffi_binding::OutOfRust>,
    c_return: &c::ClosureReturnParameter,
) -> Result<(HelperId, String)> {
    let signature = closure.signature().as_str();
    let storage = returned_storage_name(signature);
    let call_ty = c_return.call_type();
    let c::Type::FunctionPointer {
        returns: call_returns,
        params: call_params,
    } = call_ty
    else {
        return Err(Error::BrokenBridgeContract {
            bridge: DartHost::TARGET,
            invariant: "returned closure call type is not a function pointer",
        });
    };
    let invoke_native = format!(
        "{} Function({})",
        ffi::native_type(call_returns),
        call_params
            .iter()
            .map(ffi::native_type)
            .collect::<Vec<_>>()
            .join(", ")
    );
    let source = format!(
        "final class {storage} extends $$ffi.Struct {{\n  external $$ffi.Pointer<$$ffi.NativeFunction<{invoke_native}>> invoke;\n  external $$ffi.Pointer<$$ffi.Void> context;\n  external $$ffi.Pointer<$$ffi.NativeFunction<$$ffi.Void Function($$ffi.Pointer<$$ffi.Void>)>> release;\n}}\n\n"
    );
    Ok((
        HelperId::new(boltffi_binding::CanonicalName::single(format!(
            "closure_return_{signature}"
        ))),
        source,
    ))
}

/// Generate the call site that fills a returned-closure storage and wraps it.
pub fn returned_call(
    function: &Function,
    mut args: Vec<String>,
    closure: &ClosureReturn<Native, boltffi_binding::OutOfRust>,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<(String, Vec<(HelperId, String)>)> {
    let c_return = function
        .parameter_groups()
        .iter()
        .find_map(|group| match group {
            c::ParameterGroup::ClosureReturn(value) => Some(value),
            _ => None,
        })
        .ok_or(Error::BrokenBridgeContract {
            bridge: DartHost::TARGET,
            invariant: "missing C closure return parameter for Dart closure return",
        })?;
    let helper = returned_helper(closure, c_return)?;
    let storage = returned_storage_name(closure.signature().as_str());
    args.push("storage.cast()".into());

    let c::Type::FunctionPointer {
        returns: call_returns,
        params: call_params,
    } = c_return.call_type()
    else {
        return Err(Error::BrokenBridgeContract {
            bridge: DartHost::TARGET,
            invariant: "returned closure call type is not a function pointer",
        });
    };
    let dart_invoke = format!(
        "{} Function({})",
        ffi::dart_type(call_returns),
        call_params
            .iter()
            .map(ffi::dart_type)
            .collect::<Vec<_>>()
            .join(", ")
    );

    let (pack_setup, pack_cleanup, call_args, decode) =
        invoke_returned(closure, c_return, bridge, context)?;
    let call_args_joined = call_args.join(", ");
    let call_args_suffix = if call_args_joined.is_empty() {
        String::new()
    } else {
        format!(", {call_args_joined}")
    };
    let pack_body = if pack_setup.is_empty() {
        format!(
            "final invoke = owner.storage.ref.invoke.asFunction<{dart_invoke}>();\n  final raw = invoke(owner.storage.ref.context{call_args_suffix});\n  {decode}"
        )
    } else {
        format!(
            "{}\n  try {{\n    final invoke = owner.storage.ref.invoke.asFunction<{dart_invoke}>();\n    final raw = invoke(owner.storage.ref.context{call_args_suffix});\n    {decode}\n  }} finally {{\n    {}\n  }}",
            pack_setup.join("\n  "),
            pack_cleanup.join("\n    "),
        )
    };

    let nullable_check = match closure.presence() {
        HandlePresence::Nullable => {
            "if (storage.ref.invoke == $$ffi.nullptr) {\n    $$extffi.calloc.free(storage);\n    return null;\n  }\n  "
                .to_owned()
        }
        _ => {
            "if (storage.ref.invoke == $$ffi.nullptr) {\n    $$extffi.calloc.free(storage);\n    throw StateError('returned BoltFFI closure was null');\n  }\n  "
                .to_owned()
        }
    };

    let lambda_params = closure
        .invoke()
        .params()
        .iter()
        .map(|param| name_style::lower_camel(param.name()))
        .collect::<Vec<_>>()
        .join(", ");

    let body = format!(
        "final storage = $$extffi.calloc<{storage}>();\nfinal status = _f${}({});\nif (status.code != 0) {{\n  $$extffi.calloc.free(storage);\n  throw _$$FFIException(status.code, 'returned closure registration failed');\n}}\n{nullable_check}final owner = _ReturnedClosureOwner<{storage}>(\n  storage: storage,\n  context: storage.ref.context,\n  release: storage.ref.release,\n);\nreturn ({lambda_params}) {{\n  if (owner.storage.ref.invoke == $$ffi.nullptr) {{\n    throw StateError('returned BoltFFI closure was released');\n  }}\n  {pack_body}\n}};",
        function.name(),
        args.join(", "),
    );
    Ok((body, vec![helper]))
}

fn invoke_returned(
    closure: &ClosureReturn<Native, boltffi_binding::OutOfRust>,
    c_return: &c::ClosureReturnParameter,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<(Vec<String>, Vec<String>, Vec<String>, String)> {
    let mut setup = Vec::new();
    let mut cleanup = Vec::new();
    let mut args = Vec::new();
    let invoke = closure.invoke();

    // Success-out for fallible invoke returns (rare for simple counters).
    for group in c_return.parameter_groups() {
        if let c::ParameterGroup::SuccessOut(index) = group {
            let param = c_return.parameter(*index);
            let local = format!("_$out{}", param.name());
            let c::Type::MutPointer(inner) = param.ty() else {
                return Err(Error::BrokenBridgeContract {
                    bridge: DartHost::TARGET,
                    invariant: "returned-closure success out is not a pointer",
                });
            };
            setup.push(format!(
                "final {local} = $$extffi.calloc<{}>();",
                ffi::native_type(inner)
            ));
            cleanup.push(format!("$$extffi.calloc.free({local});"));
            args.push(local);
        }
    }

    let arg_groups = c_return
        .parameter_groups()
        .iter()
        .filter(|group| !matches!(group, c::ParameterGroup::SuccessOut(_)))
        .collect::<Vec<_>>();
    if arg_groups.len() != invoke.params().len() {
        return Err(Error::BrokenBridgeContract {
            bridge: DartHost::TARGET,
            invariant: "returned closure parameter group count mismatch",
        });
    }

    for (param, group) in invoke.params().iter().zip(arg_groups) {
        let IncomingParam::Value(plan) = param.payload() else {
            return unsupported("nested returned closure parameter");
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
            (ParamPlan::Encoded { codec, .. }, c::ParameterGroup::ByteSlice(_)) => {
                let writer = format!("_$writer{}", name_style::upper_camel(param.name()));
                let writes = codec
                    .render_with(&mut Writer::new(&writer, &name, context))
                    .into_iter()
                    .collect::<Result<Vec<_>>>()?
                    .join(" ");
                setup.push(format!("final {writer} = _$$WireWriter();\n  {writes}"));
                cleanup.push(format!("{writer}.close();"));
                args.extend([format!("{writer}.ptr"), format!("{writer}.len")]);
            }
            (ParamPlan::ScalarOption { primitive }, c::ParameterGroup::ByteSlice(_)) => {
                let writer = format!("_$writer{}", name_style::upper_camel(param.name()));
                let suffix = primitive::wire_suffix(*primitive)?;
                setup.push(format!(
                    "final {writer} = _$$WireWriter();\n  {writer}.writeOptional({name}, (value, writer) => writer.write{suffix}(value));"
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
                            "final {local} = $$extffi.calloc<{native}>({name}.length);\n  for (var i = 0; i < {name}.length; i++) {{ ({local} + i).value = {name}[i]; }}"
                        ));
                        cleanup.push(format!("$$extffi.calloc.free({local});"));
                        args.extend([local, format!("{name}.length")]);
                    }
                    _ => return unsupported("returned closure direct vector arg"),
                }
            }
            _ => return unsupported("returned closure argument shape"),
        }
    }

    let decode = decode_returned_invoke(invoke.returns().plan(), "raw", bridge, context)?;
    Ok((setup, cleanup, args, decode))
}

fn decode_returned_invoke(
    plan: &ReturnPlan<Native, boltffi_binding::OutOfRust>,
    raw: &str,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    let _ = bridge;
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
            _ => return unsupported("returned closure direct return"),
        },
        ReturnPlan::EncodedViaReturnSlot { codec, .. } => {
            let decode = codec.render_with(&mut Reader::new("reader", context))?;
            format!(
                "try {{\n    final reader = _$$WireReader({raw}.ptr, {raw}.len);\n    return {decode};\n  }} finally {{\n    if ({raw}.ptr != $$ffi.nullptr) _f$boltffi_free_buf({raw});\n  }}"
            )
        }
        ReturnPlan::ScalarOptionViaReturnSlot { primitive } => {
            let suffix = primitive::wire_suffix(*primitive)?;
            format!(
                "try {{\n    final reader = _$$WireReader({raw}.ptr, {raw}.len);\n    return reader.readOptional((reader) => reader.read{suffix}());\n  }} finally {{\n    if ({raw}.ptr != $$ffi.nullptr) _f$boltffi_free_buf({raw});\n  }}"
            )
        }
        _ => return unsupported("returned closure return shape"),
    })
}

fn invoke_body(
    invoke: &ImportedCallable<Native>,
    c_closure: &c::ClosureParameter,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    // p0 is always the context pointer in the C call signature.
    let mut setup = vec![
        "final impl = _map[p0.address];".into(),
        "if (impl == null) { throw StateError('released BoltFFI closure handle'); }".into(),
    ];
    let mut args = Vec::new();

    let arg_groups = c_closure
        .parameter_groups()
        .iter()
        .filter(|group| !matches!(group, c::ParameterGroup::SuccessOut(_)))
        .collect::<Vec<_>>();
    if arg_groups.len() != invoke.params().len() {
        return Err(Error::BrokenBridgeContract {
            bridge: DartHost::TARGET,
            invariant: "closure invoke parameter group count mismatch",
        });
    }

    for (param, group) in invoke.params().iter().zip(arg_groups) {
        let OutgoingParam::Value(plan) = param.payload() else {
            return unsupported("nested closure parameter");
        };
        let local = name_style::lower_camel(param.name());
        let expression = decode_param(plan, group, &local, bridge, context, &mut setup)?;
        setup.push(format!("final {local}Decoded = {expression};"));
        args.push(format!("{local}Decoded"));
    }

    let call_expr = format!("impl({})", args.join(", "));
    let success_out = c_closure.parameter_groups().iter().find_map(|group| {
        if let c::ParameterGroup::SuccessOut(index) = group {
            // +1 for leading context parameter in the trampoline.
            Some(format!("p{}", index.position() + 1))
        } else {
            None
        }
    });
    let body = sync_return(invoke, &call_expr, success_out.as_deref(), bridge, context)?;
    setup.push(body);
    Ok(setup.join("\n"))
}

fn decode_param(
    plan: &ParamPlan<Native, boltffi_binding::OutOfRust>,
    group: &c::ParameterGroup,
    local: &str,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
    setup: &mut Vec<String>,
) -> Result<String> {
    // Invoke C parameters are stored without the leading context pointer, but
    // the trampoline names every C call argument p0..pn with p0 = context.
    let p = |index: c::ParameterIndex| format!("p{}", index.position() + 1);

    Ok(match (plan, group) {
        (ParamPlan::Direct { ty, .. }, c::ParameterGroup::Value(index)) => {
            let raw = p(*index);
            match ty {
                DirectValueType::Primitive(boltffi_binding::Primitive::Bool) => raw,
                DirectValueType::Enum(id) => format!(
                    "{}._fromValue({raw})",
                    call::direct_type(&DirectValueType::Enum(*id), context)?
                ),
                DirectValueType::Record(_) => {
                    format!("{}._fromStruct({raw})", call::direct_type(ty, context)?)
                }
                _ => raw,
            }
        }
        (ParamPlan::Encoded { codec, .. }, c::ParameterGroup::ByteSlice(bytes)) => {
            let ptr = p(bytes.pointer());
            let len = p(bytes.length());
            let reader = format!("{local}Reader");
            setup.push(format!("final {reader} = _$$WireReader({ptr}, {len});"));
            codec.render_with(&mut Reader::new(&reader, context))?
        }
        (ParamPlan::ScalarOption { primitive }, c::ParameterGroup::ByteSlice(bytes)) => {
            let ptr = p(bytes.pointer());
            let len = p(bytes.length());
            let suffix = primitive::wire_suffix(*primitive)?;
            format!(
                "{len} == 0 ? null : _$$WireReader({ptr}, {len}).readOptional((reader) => reader.read{suffix}())"
            )
        }
        (ParamPlan::DirectVec { element, .. }, c::ParameterGroup::DirectVector(vector)) => {
            let ptr = p(vector.pointer());
            let len = p(vector.length());
            match element {
                DirectVectorElementType::Primitive(primitive) => {
                    let native = call::primitive_native_type(primitive.primitive())?;
                    format!(
                        "List.generate({len}, (i) => (({ptr}.cast<{native}>()) + i).value, growable: false)"
                    )
                }
                DirectVectorElementType::Record(id) => {
                    let c_record =
                        bridge
                            .source_direct_record(*id)
                            .ok_or(Error::BrokenBridgeContract {
                                bridge: DartHost::TARGET,
                                invariant: "missing closure direct vector record",
                            })?;
                    let c_name = ffi::record_name(c_record);
                    let public = context
                        .record(*id)
                        .map(|record| name_style::upper_camel(record.name()))
                        .ok_or(missing("closure direct vector record"))?;
                    format!(
                        "List.generate({len} ~/ $$ffi.sizeOf<{c_name}>(), (i) => {public}._fromStruct((({ptr}.cast<{c_name}>()) + i).ref), growable: false)"
                    )
                }
                _ => return unsupported("unknown closure direct vector"),
            }
        }
        (
            ParamPlan::Handle {
                target, presence, ..
            },
            c::ParameterGroup::Value(index),
        ) => {
            let raw = p(*index);
            let decoded = match target {
                HandleTarget::Class(id) => {
                    let name = context
                        .class(*id)
                        .map(|class| name_style::upper_camel(class.name()))
                        .ok_or(missing("closure class parameter"))?;
                    format!("{name}._({raw})")
                }
                _ => return unsupported("closure handle parameter target"),
            };
            if *presence == HandlePresence::Nullable {
                format!("{raw} == 0 ? null : {decoded}")
            } else {
                decoded
            }
        }
        _ => return unsupported("closure parameter marshalling shape"),
    })
}

fn sync_return(
    invoke: &ImportedCallable<Native>,
    call_expr: &str,
    success_out: Option<&str>,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    match invoke.error().channel() {
        ErrorChannel::None => {
            infallible_return(invoke.returns().plan(), call_expr, bridge, context)
        }
        ErrorChannel::Encoded { codec, .. } => {
            let success = success_store(
                invoke.returns().plan(),
                "value",
                success_out,
                bridge,
                context,
            )?;
            let error = codec
                .render_with(&mut Writer::new("writer", "value", context))
                .into_iter()
                .collect::<Result<Vec<_>>>()?
                .join(" ");
            Ok(format!(
                "final result = {call_expr};\nswitch (result) {{\n  case BoltFFIResult$Ok(:final value):\n    {success}\n    return _$$emptyBuf();\n  case BoltFFIResult$Err(:final value):\n    final writer = _$$WireWriter();\n    try {{\n      {error}\n      return writer.toRustBuffer();\n    }} finally {{ writer.close(); }}\n}}"
            ))
        }
        ErrorChannel::Status => unsupported("closure status error"),
        _ => unsupported("unknown closure error"),
    }
}

fn infallible_return(
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
            _ => return unsupported("closure direct return"),
        },
        ReturnPlan::EncodedViaReturnSlot { codec, .. } => {
            let writes = codec
                .render_with(&mut Writer::new("writer", "value", context))
                .into_iter()
                .collect::<Result<Vec<_>>>()?
                .join(" ");
            format!(
                "final value = {call_expr};\nfinal writer = _$$WireWriter();\ntry {{\n  {writes}\n  return writer.toRustBuffer();\n}} finally {{ writer.close(); }}"
            )
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
                vector_buffer(call_expr, element, bridge, context)?
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
                    .ok_or(missing("closure callback return"))?;
                let map = format!("_I${}HandleMap", name_style::upper_camel(callback.name()));
                if *presence == HandlePresence::Nullable {
                    format!(
                        "final value = {call_expr};\nreturn value == null ? _$$nullCallbackHandle() : {map}.createHandle(value);"
                    )
                } else {
                    format!("return {map}.createHandle({call_expr});")
                }
            }
            _ => return unsupported("closure handle return"),
        },
        _ => return unsupported("closure return shape"),
    })
}

fn success_store(
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
        invariant: "fallible closure success output missing",
    })?;
    Ok(match plan {
        ReturnPlan::DirectViaOutPointer { ty } => match ty {
            DirectValueType::Primitive(_) => format!("{out}.value = {value};"),
            DirectValueType::Enum(_) => format!("{out}.value = {value}.value;"),
            DirectValueType::Record(_) => format!("{out}.ref = {value}._toStruct();"),
            _ => return unsupported("fallible closure direct success"),
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
        ReturnPlan::ScalarOptionViaReturnSlot { primitive } => {
            let suffix = primitive::wire_suffix(*primitive)?;
            format!(
                "final writer = _$$WireWriter();\n    try {{ writer.writeOptional({value}, (value, writer) => writer.write{suffix}(value)); {out}.ref = writer.toRustBuffer(); }} finally {{ writer.close(); }}"
            )
        }
        ReturnPlan::DirectVecViaReturnSlot { element } => {
            let buffer = vector_buffer(value, element, bridge, context)?;
            format!("{buffer}\n    {out}.ref = buffer;")
        }
        _ => return unsupported("fallible closure success shape"),
    })
}

fn vector_buffer(
    value: &str,
    element: &DirectVectorElementType,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    let (native, assign) = match element {
        DirectVectorElementType::Primitive(primitive) => (
            call::primitive_native_type(primitive.primitive())?.to_owned(),
            "(raw + i).value = items[i];".to_owned(),
        ),
        DirectVectorElementType::Record(id) => {
            let c_record = bridge
                .source_direct_record(*id)
                .ok_or(missing("closure vector C record"))?;
            let native = ffi::record_name(c_record);
            let record = context
                .record(*id)
                .ok_or(missing("closure vector record"))?;
            let boltffi_binding::RecordDecl::Direct(record) = record else {
                return unsupported("encoded closure direct vector record");
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
        _ => return unsupported("unknown closure vector return"),
    };
    Ok(format!(
        "final items = {value};\nfinal buffer = _f$boltffi_buf_with_len(items.length * $$ffi.sizeOf<{native}>());\nfinal raw = buffer.ptr.cast<{native}>();\nfor (var i = 0; i < items.length; i++) {{ {assign} }}"
    ))
}

fn exceptional_return(ty: &c::Type) -> String {
    match ty {
        c::Type::Void
        | c::Type::Buffer
        | c::Type::String
        | c::Type::Span
        | c::Type::Status
        | c::Type::CallbackHandle(_)
        | c::Type::Named(_)
        | c::Type::DirectRecord(_)
        | c::Type::ConstPointer(_)
        | c::Type::MutPointer(_)
        | c::Type::FunctionPointer { .. } => String::new(),
        c::Type::Bool => ", false".into(),
        c::Type::Float32 | c::Type::Float64 => ", 0.0".into(),
        _ => ", 0".into(),
    }
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

use boltffi_binding::{
    DirectValueType, DirectVectorElementType, ErrorChannel, ExportedCallable, HandlePresence,
    HandleTarget, IncomingParam, Native, NativeSymbol, ParamPlan, Primitive, Receive, ReturnPlan,
};

use crate::{
    bridge::c::{CBridgeContract, Function},
    core::{Error, RenderContext, Result},
};

use super::{DartHost, codec::Writer, name_style, primitive, render::dart_type};

pub fn c_function<'a>(symbol: &NativeSymbol, bridge: &'a CBridgeContract) -> Result<&'a Function> {
    bridge
        .functions()
        .iter()
        .find(|function| function.source_symbol() == Some(symbol.id()))
        .ok_or(Error::BrokenBridgeContract {
            bridge: DartHost::TARGET,
            invariant: "missing C function for Dart callable",
        })
}

pub fn parameter_type(
    plan: &ParamPlan<Native, boltffi_binding::IntoRust>,
    context: &RenderContext<Native>,
) -> Result<String> {
    plan_type(plan, context)
}

pub fn outgoing_parameter_type(
    plan: &ParamPlan<Native, boltffi_binding::OutOfRust>,
    context: &RenderContext<Native>,
) -> Result<String> {
    plan_type(plan, context)
}

fn plan_type<D: boltffi_binding::Direction>(
    plan: &ParamPlan<Native, D>,
    context: &RenderContext<Native>,
) -> Result<String>
where
    D::Opposite: boltffi_binding::ParamDirection<Native>,
{
    Ok(match plan {
        ParamPlan::Direct { ty, .. } => direct_type(ty, context)?,
        ParamPlan::Encoded { ty, .. } => dart_type(ty, context)?,
        ParamPlan::Handle {
            target, presence, ..
        } => {
            let ty = handle_type(target, context)?;
            if *presence == HandlePresence::Nullable {
                format!("{ty}?")
            } else {
                ty
            }
        }
        ParamPlan::ScalarOption { primitive: value } => {
            format!("{}?", primitive::api_type(*value)?)
        }
        ParamPlan::DirectVec { element, .. } => {
            format!("List<{}>", vector_element_type(element, context)?)
        }
        _ => return unsupported("unknown Dart parameter plan"),
    })
}

pub fn return_type(
    plan: &ReturnPlan<Native, boltffi_binding::OutOfRust>,
    context: &RenderContext<Native>,
) -> Result<String> {
    return_plan_type(plan, context)
}
pub fn callback_return_type(
    plan: &ReturnPlan<Native, boltffi_binding::IntoRust>,
    context: &RenderContext<Native>,
) -> Result<String> {
    return_plan_type(plan, context)
}

fn return_plan_type<D: boltffi_binding::Direction>(
    plan: &ReturnPlan<Native, D>,
    context: &RenderContext<Native>,
) -> Result<String>
where
    D::Opposite: boltffi_binding::ParamDirection<Native>,
{
    Ok(match plan {
        ReturnPlan::Void => "void".into(),
        ReturnPlan::DirectViaReturnSlot { ty } | ReturnPlan::DirectViaOutPointer { ty } => {
            direct_type(ty, context)?
        }
        ReturnPlan::EncodedViaReturnSlot { ty, .. }
        | ReturnPlan::EncodedViaOutPointer { ty, .. } => dart_type(ty, context)?,
        ReturnPlan::HandleViaReturnSlot {
            target, presence, ..
        }
        | ReturnPlan::HandleViaOutPointer {
            target, presence, ..
        } => {
            let ty = handle_type(target, context)?;
            if *presence == HandlePresence::Nullable {
                format!("{ty}?")
            } else {
                ty
            }
        }
        ReturnPlan::ScalarOptionViaReturnSlot { primitive: value } => {
            format!("{}?", primitive::api_type(*value)?)
        }
        ReturnPlan::DirectVecViaReturnSlot { element } => {
            format!("List<{}>", vector_element_type(element, context)?)
        }
        ReturnPlan::ClosureViaOutPointer(_) => return unsupported("closure return"),
        _ => return unsupported("unknown Dart return plan"),
    })
}

pub fn exported_api_return(
    callable: &ExportedCallable<Native>,
    context: &RenderContext<Native>,
) -> Result<String> {
    let ty = return_type(callable.returns().plan(), context)?;
    Ok(if callable.execution().uses_async_execution() {
        format!("Future<{ty}>")
    } else {
        ty
    })
}

pub fn callback_api_return(
    callable: &boltffi_binding::ImportedCallable<Native>,
    context: &RenderContext<Native>,
) -> Result<String> {
    let ok = callback_return_type(callable.returns().plan(), context)?;
    let ty = match callable.error().channel() {
        ErrorChannel::None => ok,
        ErrorChannel::Encoded { ty: err, .. } => {
            format!("BoltFFIResult<{ok}, {}>", dart_type(err, context)?)
        }
        ErrorChannel::Status => return unsupported("callback status error"),
        _ => return unsupported("unknown callback error"),
    };
    Ok(if callable.execution().uses_async_execution() {
        format!("Future<{ty}>")
    } else {
        ty
    })
}

pub fn marshal_exported(
    callable: &ExportedCallable<Native>,
    receiver: Option<&str>,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let mut setup = Vec::new();
    let mut cleanup = Vec::new();
    let mut args = receiver.into_iter().map(str::to_owned).collect::<Vec<_>>();
    for param in callable.params() {
        let name = name_style::lower_camel(param.name());
        let IncomingParam::Value(plan) = param.payload() else {
            return unsupported("inline closure parameter");
        };
        match plan {
            ParamPlan::Direct { ty, .. } => args.push(match ty {
                DirectValueType::Primitive(boltffi_binding::Primitive::Bool) => {
                    format!("{name} ? true : false")
                }
                DirectValueType::Enum(_) => format!("{name}.value"),
                DirectValueType::Record(_) => format!("{name}._toStruct()"),
                _ => name,
            }),
            ParamPlan::Encoded { codec, .. } => {
                let writer = format!("_$writer{}", name_style::upper_camel(param.name()));
                let writes = codec
                    .render_with(&mut Writer::new(&writer, &name, context))
                    .into_iter()
                    .collect::<Result<Vec<_>>>()?
                    .join("\n    ");
                setup.push(format!("final {writer} = _$$WireWriter();\n    {writes}"));
                cleanup.push(format!("{writer}.close();"));
                args.extend([format!("{writer}.ptr"), format!("{writer}.len")]);
            }
            ParamPlan::Handle {
                target, presence, ..
            } => args.push(match target {
                HandleTarget::Class(_) => {
                    if *presence == HandlePresence::Nullable {
                        format!("{name}?._rawHandle ?? 0")
                    } else {
                        format!("{name}._rawHandle")
                    }
                }
                HandleTarget::Callback(id) => {
                    let callback = context.callback(*id).ok_or(Error::BrokenBridgeContract {
                        bridge: DartHost::TARGET,
                        invariant: "missing callback parameter type",
                    })?;
                    let map = format!("_I${}HandleMap", name_style::upper_camel(callback.name()));
                    if *presence == HandlePresence::Nullable {
                        format!(
                            "{name} == null ? _$$nullCallbackHandle() : {map}.createHandle({name})"
                        )
                    } else {
                        format!("{map}.createHandle({name})")
                    }
                }
                _ => return unsupported("stream handle parameter"),
            }),
            ParamPlan::DirectVec { element, receive } => {
                let local = format!("_$vector{}", name_style::upper_camel(param.name()));
                match element {
                    DirectVectorElementType::Primitive(value) => {
                        let native = primitive_native_type(value.primitive())?;
                        setup.push(format!(
                            "final {local} = $$extffi.calloc<{native}>({name}.length);\n    for (var i = 0; i < {name}.length; i++) {{ {local}.elementAt(i).value = {name}[i]; }}"
                        ));
                        if *receive == Receive::ByMutRef {
                            cleanup.push(format!(
                                "for (var i = 0; i < {name}.length; i++) {{ {name}[i] = {local}.elementAt(i).value; }}"
                            ));
                        }
                        cleanup.push(format!("$$extffi.calloc.free({local});"));
                        args.extend([local, format!("{name}.length")]);
                    }
                    DirectVectorElementType::Record(id) => {
                        if *receive == Receive::ByMutRef {
                            return unsupported("mutable direct-record vector parameter");
                        }
                        let record = context.record(*id).ok_or(Error::BrokenBridgeContract {
                            bridge: DartHost::TARGET,
                            invariant: "missing direct vector record",
                        })?;
                        let boltffi_binding::RecordDecl::Direct(record) = record else {
                            return unsupported("encoded direct-vector record");
                        };
                        let c_record = bridge.source_direct_record(*id).ok_or(
                            Error::BrokenBridgeContract {
                                bridge: DartHost::TARGET,
                                invariant: "missing C direct vector record",
                            },
                        )?;
                        let c_name = super::ffi::record_name(c_record);
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
                            "final {local} = $$extffi.calloc<{c_name}>({name}.length);\n    for (var i = 0; i < {name}.length; i++) {{ final value = {name}[i]; final target = {local}.elementAt(i).ref; {copies} }}"
                        ));
                        cleanup.push(format!("$$extffi.calloc.free({local});"));
                        args.extend([
                            format!("{local}.cast<$$ffi.Uint8>()"),
                            format!("{name}.length * $$ffi.sizeOf<{c_name}>()"),
                        ]);
                    }
                    _ => return unsupported("unknown direct vector argument"),
                }
            }
            ParamPlan::ScalarOption { primitive } => {
                let writer = format!("_$writer{}", name_style::upper_camel(param.name()));
                let suffix = primitive::wire_suffix(*primitive)?;
                setup.push(format!(
                    "final {writer} = _$$WireWriter();\n    {writer}.writeOptional({name}, (value, writer) => writer.write{suffix}(value));"
                ));
                cleanup.push(format!("{writer}.close();"));
                args.extend([format!("{writer}.ptr"), format!("{writer}.len")]);
            }
            _ => return unsupported("unknown argument marshalling"),
        }
    }
    Ok((setup, cleanup, args))
}

pub fn primitive_native_type(primitive: Primitive) -> Result<&'static str> {
    match primitive {
        Primitive::Bool => Ok("$$ffi.Bool"),
        Primitive::I8 => Ok("$$ffi.Int8"),
        Primitive::U8 => Ok("$$ffi.Uint8"),
        Primitive::I16 => Ok("$$ffi.Int16"),
        Primitive::U16 => Ok("$$ffi.Uint16"),
        Primitive::I32 => Ok("$$ffi.Int32"),
        Primitive::U32 => Ok("$$ffi.Uint32"),
        Primitive::I64 => Ok("$$ffi.Int64"),
        Primitive::U64 => Ok("$$ffi.Uint64"),
        Primitive::ISize => Ok("$$ffi.IntPtr"),
        Primitive::USize => Ok("$$ffi.UintPtr"),
        Primitive::F32 => Ok("$$ffi.Float"),
        Primitive::F64 => Ok("$$ffi.Double"),
        _ => unsupported("unknown direct vector primitive"),
    }
}

pub fn direct_type(ty: &DirectValueType, context: &RenderContext<Native>) -> Result<String> {
    match ty {
        DirectValueType::Primitive(value) => primitive::api_type(*value).map(str::to_owned),
        DirectValueType::Record(id) => context
            .record(*id)
            .map(|decl| name_style::upper_camel(decl.name()))
            .ok_or(Error::BrokenBridgeContract {
                bridge: DartHost::TARGET,
                invariant: "missing direct record",
            }),
        DirectValueType::Enum(id) => context
            .enumeration(*id)
            .map(|decl| name_style::upper_camel(decl.name()))
            .ok_or(Error::BrokenBridgeContract {
                bridge: DartHost::TARGET,
                invariant: "missing direct enum",
            }),
        _ => unsupported("unknown direct type"),
    }
}

fn handle_type(target: &HandleTarget, context: &RenderContext<Native>) -> Result<String> {
    match target {
        HandleTarget::Class(id) => context
            .class(*id)
            .map(|decl| name_style::upper_camel(decl.name()))
            .ok_or(Error::BrokenBridgeContract {
                bridge: DartHost::TARGET,
                invariant: "missing class",
            }),
        HandleTarget::Callback(id) => context
            .callback(*id)
            .map(|decl| name_style::upper_camel(decl.name()))
            .ok_or(Error::BrokenBridgeContract {
                bridge: DartHost::TARGET,
                invariant: "missing callback",
            }),
        HandleTarget::Stream(_) => unsupported("stream handle type"),
        _ => unsupported("unknown handle type"),
    }
}

fn vector_element_type(
    element: &DirectVectorElementType,
    context: &RenderContext<Native>,
) -> Result<String> {
    match element {
        DirectVectorElementType::Primitive(value) => {
            primitive::api_type(value.primitive()).map(str::to_owned)
        }
        DirectVectorElementType::Record(id) => context
            .record(*id)
            .map(|decl| name_style::upper_camel(decl.name()))
            .ok_or(Error::BrokenBridgeContract {
                bridge: DartHost::TARGET,
                invariant: "missing vector record",
            }),
        _ => unsupported("unknown vector element"),
    }
}

fn unsupported<T>(shape: &'static str) -> Result<T> {
    Err(Error::UnsupportedTarget {
        target: DartHost::TARGET,
        shape,
    })
}

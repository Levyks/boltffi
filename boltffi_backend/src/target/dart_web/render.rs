//! Thin js_interop wrapper generation.
//!
//! This target does NOT reimplement the wasm ABI (pointer/writer-level
//! marshalling) the way target::dart (dart:ffi) and target::typescript do.
//! It generates Dart bindings that call the ALREADY-GENERATED,
//! ALREADY-WORKING JS module that target::typescript produces for the same
//! crate: `@JS()` external declarations bound to that module's exports,
//! wrapped in plain-Dart-typed functions/classes matching the native
//! dart:ffi target's public surface as closely as possible, so a single
//! implementation/call site works unchanged on both.

use boltffi_binding::{
    BuiltinType, CallbackDecl, ClassDecl, DataVariantPayload, DirectValueType, EnumDecl,
    ErrorChannel, ExecutionDecl, ExportedCallable, FunctionDecl, HandlePresence, HandleTarget,
    IncomingParam, ParamPlan, Primitive, RecordDecl, ReturnPlan, TypeRef, Wasm32,
};

use crate::core::{Emitted, Error, RenderContext, Result, name_case};

use super::name_style;

/// The JS-facing name for a declaration, as `target::typescript` spells it
/// -- plain camelCase with no keyword escaping. `name_style::lower_camel`
/// (the Dart-facing name) escapes Dart keywords (`new` -> `new_`, `get` ->
/// `get_`, ...); using that escaped spelling to address the compiled JS
/// module's properties is a name mismatch (JS doesn't reserve `new`/`get`
/// as property names) that leaves those members silently unreachable.
fn js_name(name: &boltffi_binding::CanonicalName) -> String {
    name_case::lower_camel(name)
}

/// Public Dart type name for a general `TypeRef`.
pub fn dart_type(ty: &TypeRef, context: &RenderContext<Wasm32>) -> Result<String> {
    Ok(match ty {
        TypeRef::Primitive(Primitive::Bool) => "bool".to_owned(),
        TypeRef::Primitive(
            Primitive::I8
            | Primitive::U8
            | Primitive::I16
            | Primitive::U16
            | Primitive::I32
            | Primitive::U32
            | Primitive::I64
            | Primitive::U64
            | Primitive::ISize
            | Primitive::USize,
        ) => "int".to_owned(),
        TypeRef::Primitive(Primitive::F32 | Primitive::F64) => "double".to_owned(),
        TypeRef::String | TypeRef::InternedString { .. } => "String".to_owned(),
        TypeRef::Bytes => "$$typed_data.Uint8List".to_owned(),
        TypeRef::Record(id) => context
            .record(*id)
            .map(|decl| name_style::upper_camel(decl.name()))
            .ok_or_else(|| missing("record"))?,
        TypeRef::Enum(id) => context
            .enumeration(*id)
            .map(|decl| name_style::upper_camel(decl.name()))
            .ok_or_else(|| missing("enum"))?,
        TypeRef::Class(id) => context
            .class(*id)
            .map(|decl| name_style::upper_camel(decl.name()))
            .ok_or_else(|| missing("class"))?,
        TypeRef::Callback(id) => context
            .callback(*id)
            .map(|decl| name_style::upper_camel(decl.name()))
            .ok_or_else(|| missing("callback"))?,
        TypeRef::Builtin(BuiltinType::Duration) => "Duration".to_owned(),
        TypeRef::Builtin(BuiltinType::SystemTime) => "DateTime".to_owned(),
        TypeRef::Builtin(BuiltinType::Uuid | BuiltinType::Url) => "String".to_owned(),
        TypeRef::Optional(inner) => format!("{}?", dart_type(inner, context)?),
        TypeRef::Sequence(inner) => format!("List<{}>", dart_type(inner, context)?),
        _ => return unsupported("dart_web type"),
    })
}

/// Converts a Dart-valued expression into a JS-valued expression suitable
/// to pass as a function argument or as an object property value.
pub fn to_js(expr: &str, ty: &TypeRef, context: &RenderContext<Wasm32>) -> Result<String> {
    Ok(match ty {
        TypeRef::Primitive(Primitive::Bool) => format!("({expr}).toJS"),
        TypeRef::Primitive(
            Primitive::I8
            | Primitive::U8
            | Primitive::I16
            | Primitive::U16
            | Primitive::I32
            | Primitive::U32,
        ) => format!("({expr}).toDouble().toJS"),
        TypeRef::Primitive(
            Primitive::I64 | Primitive::U64 | Primitive::ISize | Primitive::USize,
        ) => format!("$$dartIntToJSBigInt({expr})"),
        TypeRef::Primitive(Primitive::F32 | Primitive::F64) => format!("({expr}).toJS"),
        TypeRef::String | TypeRef::InternedString { .. } => format!("({expr}).toJS"),
        TypeRef::Bytes => format!("({expr}).toJS"),
        TypeRef::Record(id) => {
            record_type_name(*id, context)?;
            format!("({expr})._toJS()")
        }
        TypeRef::Enum(id) => {
            enum_type_name(*id, context)?;
            format!("({expr})._toJS()")
        }
        TypeRef::Class(id) => {
            class_type_name(*id, context)?;
            format!("({expr})._toJS()")
        }
        TypeRef::Callback(id) => {
            let callback = context.callback(*id).ok_or_else(|| missing("callback"))?;
            let name = name_style::upper_camel(callback.name());
            // The JS side takes the interface object directly (its own
            // `registerXxx` bookkeeping happens internally); Dart only
            // needs to hand over the js_interop-wrapped adapter.
            format!("$$js.createJSInteropWrapper(_{name}JSAdapter({expr}))")
        }
        TypeRef::Builtin(BuiltinType::Duration) => format!("$$durationToJS({expr})"),
        TypeRef::Builtin(BuiltinType::Uuid | BuiltinType::Url) => format!("({expr}).toJS"),
        TypeRef::Optional(inner) => {
            let converted = to_js("__boltffiOpt", inner, context)?;
            format!(
                "(({expr}) == null ? null : (((() {{ final __boltffiOpt = ({expr})!; return {converted}; }})())))"
            )
        }
        TypeRef::Sequence(inner) => {
            let converted = to_js("__boltffiElement", inner, context)?;
            format!("({expr}).map((__boltffiElement) => {converted}).toList().toJS")
        }
        _ => return unsupported("dart_web to_js type"),
    })
}

/// Converts a JS-valued expression into a Dart-valued expression.
pub fn from_js(expr: &str, ty: &TypeRef, context: &RenderContext<Wasm32>) -> Result<String> {
    Ok(match ty {
        TypeRef::Primitive(Primitive::Bool) => format!("({expr} as $$js.JSBoolean).toDart"),
        TypeRef::Primitive(
            Primitive::I8
            | Primitive::U8
            | Primitive::I16
            | Primitive::U16
            | Primitive::I32
            | Primitive::U32,
        ) => format!("({expr} as $$js.JSNumber).toDartInt"),
        TypeRef::Primitive(
            Primitive::I64 | Primitive::U64 | Primitive::ISize | Primitive::USize,
        ) => format!("$$jsBigIntToDartInt({expr} as $$js.JSAny)"),
        TypeRef::Primitive(Primitive::F32 | Primitive::F64) => {
            format!("({expr} as $$js.JSNumber).toDartDouble")
        }
        TypeRef::String | TypeRef::InternedString { .. } => {
            format!("({expr} as $$js.JSString).toDart")
        }
        TypeRef::Bytes => format!("({expr} as $$js.JSUint8Array).toDart"),
        TypeRef::Record(id) => {
            let name = record_type_name(*id, context)?;
            format!("{name}._fromJS({expr} as $$js.JSObject)")
        }
        TypeRef::Enum(id) => {
            let name = enum_type_name(*id, context)?;
            // C-style enums cross as a bare number (`_fromJS` takes
            // `JSAny`, casting to `JSNumber` internally); data enums cross
            // as a tagged object (`_fromJS` takes `JSObject` directly).
            let cast = match context.enumeration(*id) {
                Some(EnumDecl::CStyle(_)) => "JSAny",
                _ => "JSObject",
            };
            format!("{name}._fromJS({expr} as $$js.{cast})")
        }
        TypeRef::Class(id) => {
            let name = class_type_name(*id, context)?;
            format!("{name}._fromJS({expr} as $$js.JSObject)")
        }
        TypeRef::Builtin(BuiltinType::Duration) => {
            format!("$$durationFromJS({expr} as $$js.JSObject)")
        }
        TypeRef::Builtin(BuiltinType::Uuid | BuiltinType::Url) => {
            format!("({expr} as $$js.JSString).toDart")
        }
        TypeRef::Optional(inner) => {
            let converted = from_js("__boltffiOpt", inner, context)?;
            format!(
                "(({expr}) == null ? null : (((() {{ final __boltffiOpt = ({expr})!; return {converted}; }})())))"
            )
        }
        TypeRef::Sequence(inner) => {
            let converted = from_js("__boltffiElement", inner, context)?;
            format!(
                "({expr} as $$js.JSArray).toDart.map((__boltffiElement) => {converted}).toList()"
            )
        }
        _ => return unsupported("dart_web from_js type"),
    })
}

fn record_type_name(
    id: boltffi_binding::RecordId,
    context: &RenderContext<Wasm32>,
) -> Result<String> {
    context
        .record(id)
        .map(|decl| name_style::upper_camel(decl.name()))
        .ok_or_else(|| missing("record"))
}

fn enum_type_name(id: boltffi_binding::EnumId, context: &RenderContext<Wasm32>) -> Result<String> {
    context
        .enumeration(id)
        .map(|decl| name_style::upper_camel(decl.name()))
        .ok_or_else(|| missing("enum"))
}

fn class_type_name(
    id: boltffi_binding::ClassId,
    context: &RenderContext<Wasm32>,
) -> Result<String> {
    context
        .class(id)
        .map(|decl| name_style::upper_camel(decl.name()))
        .ok_or_else(|| missing("class"))
}

/// JS-side property name for a field: matches what target::typescript's
/// generated codec actually uses (`value{N}` for positional/tuple fields,
/// not `field{N}`), independent of the Dart-facing name.
fn js_property_name(key: &boltffi_binding::FieldKey) -> String {
    match key {
        boltffi_binding::FieldKey::Named(name) => name_style::lower_camel(name),
        boltffi_binding::FieldKey::Position(position) => format!("value{position}"),
        _ => unreachable!("unknown field key"),
    }
}

struct RecordField {
    dart_name: String,
    js_name: String,
    ty: TypeRef,
}

fn record_fields_generic(
    fields: impl Iterator<Item = (boltffi_binding::FieldKey, TypeRef)>,
) -> Vec<RecordField> {
    fields
        .map(|(key, ty)| RecordField {
            dart_name: name_style::field(&key),
            js_name: js_property_name(&key),
            ty,
        })
        .collect()
}

pub fn record(decl: &RecordDecl<Wasm32>, context: &RenderContext<Wasm32>) -> Result<Emitted> {
    let (name, fields, is_error) = match decl {
        RecordDecl::Direct(record) => (
            name_style::upper_camel(record.name()),
            record_fields_generic(record.fields().iter().map(|field| {
                (
                    field.key().clone(),
                    TypeRef::Primitive(field.ty().primitive()),
                )
            })),
            record.is_error_payload(),
        ),
        RecordDecl::Encoded(record) => (
            name_style::upper_camel(record.name()),
            record_fields_generic(
                record
                    .fields()
                    .iter()
                    .map(|field| (field.key().clone(), field.ty().clone())),
            ),
            record.is_error_payload(),
        ),
        _ => return unsupported("dart_web record declaration"),
    };
    render_record_class(&name, &fields, is_error, context)
}

fn render_record_class(
    name: &str,
    fields: &[RecordField],
    is_error: bool,
    context: &RenderContext<Wasm32>,
) -> Result<Emitted> {
    let exception = if is_error {
        " implements Exception"
    } else {
        ""
    };
    let declarations = fields
        .iter()
        .map(|field| {
            Ok(format!(
                "  final {} {};",
                dart_type(&field.ty, context)?,
                field.dart_name
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join("\n");
    let parameters = fields
        .iter()
        .map(|field| format!("required this.{}", field.dart_name))
        .collect::<Vec<_>>()
        .join(", ");
    let from_js = fields
        .iter()
        .map(|field| {
            Ok(format!(
                "    {}: {},",
                field.dart_name,
                from_js(
                    &format!("_js.getProperty('{}'.toJS)", field.js_name),
                    &field.ty,
                    context,
                )?
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join("\n");
    let to_js = fields
        .iter()
        .map(|field| {
            Ok(format!(
                "    _js.setProperty('{}'.toJS, {});",
                field.js_name,
                to_js(&format!("this.{}", field.dart_name), &field.ty, context)?
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join("\n");
    Ok(Emitted::primary(format!(
        "final class {name}{exception} {{\n{declarations}\n\n  const {name}({{{parameters}}});\n\n  factory {name}._fromJS($$js.JSObject _js) => {name}(\n{from_js}\n  );\n\n  $$js.JSObject _toJS() {{\n    final _js = $$js.JSObject();\n{to_js}\n    return _js;\n  }}\n}}\n\n"
    )))
}

pub fn enumeration(decl: &EnumDecl<Wasm32>, context: &RenderContext<Wasm32>) -> Result<Emitted> {
    match decl {
        EnumDecl::CStyle(value) => {
            let name = name_style::upper_camel(value.name());
            let exception = if value.is_error_payload() {
                " implements Exception"
            } else {
                ""
            };
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
                "final class {name}{exception} {{\n  final int value;\n  final String name;\n  const {name}._(this.value, this.name);\n\n{variants}\n  static const values = <{name}>[{values}];\n  static {name} _fromValue(int value) => values.firstWhere((item) => item.value == value, orElse: () => throw StateError('Unknown {name} value: $value'));\n  factory {name}._fromJS($$js.JSAny js) => _fromValue((js as $$js.JSNumber).toDartInt);\n  $$js.JSAny _toJS() => value.toDouble().toJS;\n  @override String toString() => name;\n}}\n\n"
            )))
        }
        EnumDecl::Data(value) => data_enum(value, context),
        _ => unsupported("dart_web enum declaration"),
    }
}

fn data_enum(
    value: &boltffi_binding::DataEnumDecl<Wasm32>,
    context: &RenderContext<Wasm32>,
) -> Result<Emitted> {
    let name = name_style::upper_camel(value.name());
    let exception = if value.is_error_payload() {
        " implements Exception"
    } else {
        ""
    };
    let mut classes = Vec::new();
    let mut factories = Vec::new();
    let mut from_js_cases = Vec::new();
    for variant in value.variants() {
        let variant_name = format!("{name}${}", name_style::upper_camel(variant.name()));
        let factory_name = name_style::lower_camel(variant.name());
        // Must match target::typescript's own tag convention exactly
        // (PascalCase variant name, e.g. `{tag: "NotConfigured"}`).
        let js_tag = name_style::upper_camel(variant.name());
        let fields = match variant.payload() {
            DataVariantPayload::Unit => &[][..],
            DataVariantPayload::Tuple(fields) | DataVariantPayload::Struct(fields) => {
                fields.as_slice()
            }
            _ => return unsupported("dart_web data enum payload"),
        };
        let record_fields = record_fields_generic(
            fields
                .iter()
                .map(|field| (field.key().clone(), field.ty().clone())),
        );
        let declarations = record_fields
            .iter()
            .map(|field| {
                Ok(format!(
                    "  final {} {};",
                    dart_type(&field.ty, context)?,
                    field.dart_name
                ))
            })
            .collect::<Result<Vec<_>>>()?
            .join("\n");
        let (factory_params, ctor_params) = match variant.payload() {
            DataVariantPayload::Unit => (String::new(), String::new()),
            DataVariantPayload::Tuple(_) => (
                record_fields
                    .iter()
                    .map(|field| {
                        Ok(format!(
                            "{} {}",
                            dart_type(&field.ty, context)?,
                            field.dart_name
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?
                    .join(", "),
                record_fields
                    .iter()
                    .map(|field| format!("this.{}", field.dart_name))
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            DataVariantPayload::Struct(_) => (
                format!(
                    "{{{}}}",
                    record_fields
                        .iter()
                        .map(|field| Ok(format!(
                            "required {} {}",
                            dart_type(&field.ty, context)?,
                            field.dart_name
                        )))
                        .collect::<Result<Vec<_>>>()?
                        .join(", ")
                ),
                format!(
                    "{{{}}}",
                    record_fields
                        .iter()
                        .map(|field| format!("required this.{}", field.dart_name))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ),
            _ => return unsupported("dart_web data enum payload"),
        };
        factories.push(format!(
            "  const factory {name}.{factory_name}({factory_params}) = {variant_name};"
        ));
        let ctor = format!("const {variant_name}({ctor_params})");
        let to_js_fields = record_fields
            .iter()
            .map(|field| {
                Ok(format!(
                    "    _js.setProperty('{}'.toJS, {});",
                    field.js_name,
                    to_js(&format!("this.{}", field.dart_name), &field.ty, context)?
                ))
            })
            .collect::<Result<Vec<_>>>()?
            .join("\n");
        classes.push(format!(
            "final class {variant_name} extends {name} {{\n{declarations}\n  {ctor};\n  @override\n  $$js.JSObject _toJS() {{\n    final _js = $$js.JSObject();\n    _js.setProperty('tag'.toJS, '{js_tag}'.toJS);\n{to_js_fields}\n    return _js;\n  }}\n}}"
        ));
        // Tuple variant constructors take positional args (`field0`); only
        // struct variants take named ones -- matches the constructors
        // generated just above (`ctor_params`).
        let named = matches!(variant.payload(), DataVariantPayload::Struct(_));
        let from_js_fields = record_fields
            .iter()
            .map(|field| {
                let value = from_js(
                    &format!("_js.getProperty('{}'.toJS)", field.js_name),
                    &field.ty,
                    context,
                )?;
                Ok(if named {
                    format!("{}: {value}", field.dart_name)
                } else {
                    value
                })
            })
            .collect::<Result<Vec<_>>>()?
            .join(", ");
        from_js_cases.push(format!(
            "      '{js_tag}' => {variant_name}({from_js_fields}),"
        ));
    }
    Ok(Emitted::primary(format!(
        "sealed class {name}{exception} {{\n  const {name}();\n{}\n  factory {name}._fromJS($$js.JSObject _js) {{\n    final __boltffiTag = (_js.getProperty('tag'.toJS) as $$js.JSString).toDart;\n    return switch (__boltffiTag) {{\n{}\n      final tag => throw StateError('Unknown {name} tag: $tag'),\n    }};\n  }}\n  $$js.JSObject _toJS();\n}}\n\n{}\n\n",
        factories.join("\n"),
        from_js_cases.join("\n"),
        classes.join("\n\n")
    )))
}

struct CallbackMethodShape {
    dart_name: String,
    params: Vec<(String, TypeRef)>,
    success_ty: Option<TypeRef>,
    error_ty: Option<TypeRef>,
    asynchronous: bool,
}

fn callback_method_shape(
    method: &boltffi_binding::ImportedMethodDecl<Wasm32, boltffi_binding::ImportSymbol>,
    context: &RenderContext<Wasm32>,
) -> Result<CallbackMethodShape> {
    let callable = method.callable();
    let params = callable
        .params()
        .iter()
        .map(|param| {
            let ty = imported_param_type(param, context)?;
            Ok((name_style::lower_camel(param.name()), ty))
        })
        .collect::<Result<Vec<_>>>()?;
    let success_ty = imported_return_type(callable.returns().plan(), context)?;
    let error_ty = match callable.error().channel() {
        ErrorChannel::None => None,
        ErrorChannel::Encoded { ty, .. } => Some(ty.clone()),
        _ => return unsupported("dart_web callback error channel"),
    };
    let asynchronous = matches!(callable.execution(), ExecutionDecl::Asynchronous(_));
    Ok(CallbackMethodShape {
        dart_name: name_style::lower_camel(method.name()),
        params,
        success_ty,
        error_ty,
        asynchronous,
    })
}

fn imported_param_type(
    param: &boltffi_binding::ParamDecl<Wasm32, boltffi_binding::OutOfRust>,
    context: &RenderContext<Wasm32>,
) -> Result<TypeRef> {
    match param.payload() {
        boltffi_binding::OutgoingParam::Value(plan) => param_plan_type(plan, context),
        _ => unsupported("dart_web callback closure parameter"),
    }
}

fn param_plan_type<D: boltffi_binding::Direction>(
    plan: &ParamPlan<Wasm32, D>,
    context: &RenderContext<Wasm32>,
) -> Result<TypeRef>
where
    D::Opposite: boltffi_binding::ParamDirection<Wasm32>,
{
    Ok(match plan {
        ParamPlan::Direct { ty, .. } => direct_value_type_ref(ty),
        ParamPlan::Encoded { ty, .. } => ty.clone(),
        ParamPlan::Handle {
            target, presence, ..
        } => handle_type_ref(target, *presence, context)?,
        ParamPlan::ScalarOption { primitive } => {
            TypeRef::Optional(Box::new(TypeRef::Primitive(*primitive)))
        }
        ParamPlan::DirectVec { .. } => return unsupported("dart_web direct vector parameter"),
        _ => return unsupported("dart_web param plan"),
    })
}

fn direct_value_type_ref(ty: &DirectValueType) -> TypeRef {
    match ty {
        DirectValueType::Primitive(primitive) => TypeRef::Primitive(*primitive),
        DirectValueType::Record(id) => TypeRef::Record(*id),
        DirectValueType::Enum(id) => TypeRef::Enum(*id),
        _ => unreachable!("unknown direct value type"),
    }
}

fn handle_type_ref(
    target: &HandleTarget,
    presence: HandlePresence,
    _context: &RenderContext<Wasm32>,
) -> Result<TypeRef> {
    let base = match target {
        HandleTarget::Class(id) => TypeRef::Class(*id),
        HandleTarget::Callback(id) => TypeRef::Callback(*id),
        HandleTarget::Stream(_) => return unsupported("dart_web stream handle"),
        _ => return unsupported("dart_web handle target"),
    };
    Ok(match presence {
        HandlePresence::Required => base,
        HandlePresence::Nullable => TypeRef::Optional(Box::new(base)),
        _ => return unsupported("dart_web handle presence"),
    })
}

/// Returns the logical success type, or `None` for a void return.
fn imported_return_type<D: boltffi_binding::Direction>(
    plan: &ReturnPlan<Wasm32, D>,
    context: &RenderContext<Wasm32>,
) -> Result<Option<TypeRef>>
where
    D::Opposite: boltffi_binding::ParamDirection<Wasm32>,
{
    Ok(match plan {
        ReturnPlan::Void => None,
        ReturnPlan::DirectViaReturnSlot { ty } | ReturnPlan::DirectViaOutPointer { ty } => {
            Some(direct_value_type_ref(ty))
        }
        ReturnPlan::EncodedViaReturnSlot { ty, .. }
        | ReturnPlan::EncodedViaOutPointer { ty, .. } => Some(ty.clone()),
        ReturnPlan::ScalarOptionViaReturnSlot { primitive } => {
            Some(TypeRef::Optional(Box::new(TypeRef::Primitive(*primitive))))
        }
        ReturnPlan::HandleViaReturnSlot {
            target, presence, ..
        }
        | ReturnPlan::HandleViaOutPointer {
            target, presence, ..
        } => Some(handle_type_ref(target, *presence, context)?),
        ReturnPlan::DirectVecViaReturnSlot { .. } => {
            return unsupported("dart_web direct vector return");
        }
        _ => return unsupported("dart_web return plan"),
    })
}

/// Public return type (`Future<T>`, `Future<BoltFFIResult<T,E>>`, or the
/// bare sync equivalents) for a callback-trait method the app implements.
fn callback_public_return_type(
    shape: &CallbackMethodShape,
    context: &RenderContext<Wasm32>,
) -> Result<String> {
    let value_ty = match (&shape.success_ty, &shape.error_ty) {
        (Some(ok), Some(err)) => format!(
            "BoltFFIResult<{}, {}>",
            dart_type(ok, context)?,
            dart_type(err, context)?
        ),
        (None, Some(err)) => format!("BoltFFIResult<void, {}>", dart_type(err, context)?),
        (Some(ok), None) => dart_type(ok, context)?,
        (None, None) => "void".to_owned(),
    };
    Ok(if shape.asynchronous {
        format!("Future<{value_ty}>")
    } else {
        value_ty
    })
}

pub fn callback(decl: &CallbackDecl<Wasm32>, context: &RenderContext<Wasm32>) -> Result<Emitted> {
    let name = name_style::upper_camel(decl.name());
    let methods = decl
        .protocol()
        .methods()
        .iter()
        .map(|method| callback_method_shape(method, context))
        .collect::<Result<Vec<_>>>()?;

    let interface_methods = methods
        .iter()
        .map(|method| {
            let params = method
                .params
                .iter()
                .map(|(name, ty)| Ok(format!("{} {}", dart_type(ty, context)?, name)))
                .collect::<Result<Vec<_>>>()?
                .join(", ");
            Ok(format!(
                "  {} {}({});",
                callback_public_return_type(method, context)?,
                method.dart_name,
                params
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join("\n\n");

    let adapter_methods = methods
        .iter()
        .map(|method| render_adapter_method(method, context))
        .collect::<Result<Vec<_>>>()?
        .join("\n\n");

    Ok(Emitted::primary(format!(
        "abstract interface class {name} {{\n{interface_methods}\n}}\n\n@$$js.JSExport()\nfinal class _{name}JSAdapter {{\n  final {name} _impl;\n\n  _{name}JSAdapter(this._impl);\n\n{adapter_methods}\n}}\n\n"
    )))
}

fn render_adapter_method(
    method: &CallbackMethodShape,
    context: &RenderContext<Wasm32>,
) -> Result<String> {
    let js_params = method
        .params
        .iter()
        .enumerate()
        .map(|(index, _)| format!("$$js.JSAny? __boltffiArg{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let call_args = method
        .params
        .iter()
        .enumerate()
        .map(|(index, (_, ty))| from_js(&format!("__boltffiArg{index}"), ty, context))
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    let call = format!("_impl.{}({call_args})", method.dart_name);
    let invoke = if method.asynchronous {
        format!("await {call}")
    } else {
        call
    };

    let success_expr = |value_expr: String| -> Result<String> {
        match &method.success_ty {
            None => Ok(value_expr),
            Some(ty) => Ok(to_js(&value_expr, ty, context)?),
        }
    };

    let body = if let Some(error_ty) = &method.error_ty {
        let ok_value = match &method.success_ty {
            Some(_) => "__boltffiValue".to_owned(),
            None => "null".to_owned(),
        };
        let ok_wrapped = success_expr(ok_value)?;
        let err_wrapped = to_js("__boltffiError", error_ty, context)?;
        format!(
            "final __boltffiResult = {invoke};\n    return switch (__boltffiResult) {{\n      BoltFFIResult$Ok(value: final __boltffiValue) => $$wireOk({ok_wrapped}),\n      BoltFFIResult$Err(value: final __boltffiError) => $$wireErr({err_wrapped}),\n    }};"
        )
    } else {
        // Non-fallible methods: the compiled TS module's callback glue
        // (`callback.ts`'s `method.fallible == None` branch) reads this
        // return value directly, with no `matchWireResult`/wireOk unwrapping
        // -- only a `Result`-returning method's completion path expects the
        // `{tag: 'ok'|'err', ...}` wire shape (see the `error_ty.is_some()`
        // branch above). Wrapping here regardless of fallibility (as this
        // used to) fed the compiled module a wrapped object where it
        // expected a bare value, corrupting every non-fallible callback
        // method's result (e.g. a `String` return decoding as
        // "[object Object]").
        match &method.success_ty {
            None => format!("{invoke};\n    return null;"),
            Some(_) => {
                let wrapped = success_expr(format!("({invoke})"))?;
                format!("return {wrapped};")
            }
        }
    };

    if method.asynchronous {
        Ok(format!(
            "  $$js.JSPromise<$$js.JSAny?> {}({js_params}) {{\n    Future<$$js.JSAny?> __boltffiRun() async {{\n      {body}\n    }}\n    return __boltffiRun().toJS;\n  }}",
            method.dart_name
        ))
    } else {
        Ok(format!(
            "  $$js.JSAny? {}({js_params}) {{\n    {body}\n  }}",
            method.dart_name
        ))
    }
}

struct ExportedShape {
    dart_name: String,
    js_name: String,
    params: Vec<(String, TypeRef)>,
    success_ty: Option<TypeRef>,
    error_ty: Option<TypeRef>,
    asynchronous: bool,
}

fn exported_callable_shape(
    dart_name: String,
    js_name: String,
    callable: &ExportedCallable<Wasm32>,
    context: &RenderContext<Wasm32>,
) -> Result<ExportedShape> {
    let params = callable
        .params()
        .iter()
        .map(|param| {
            let ty = match param.payload() {
                IncomingParam::Value(plan) => param_plan_type(plan, context)?,
                _ => return unsupported("dart_web exported closure parameter"),
            };
            Ok((name_style::lower_camel(param.name()), ty))
        })
        .collect::<Result<Vec<_>>>()?;
    let success_ty = imported_return_type(callable.returns().plan(), context)?;
    let error_ty = match callable.error().channel() {
        ErrorChannel::None => None,
        ErrorChannel::Encoded { ty, .. } => Some(ty.clone()),
        _ => return unsupported("dart_web exported error channel"),
    };
    let asynchronous = matches!(callable.execution(), ExecutionDecl::Asynchronous(_));
    Ok(ExportedShape {
        dart_name,
        js_name,
        params,
        success_ty,
        error_ty,
        asynchronous,
    })
}

/// Renders the `@JS()` external declaration plus a plain-Dart-typed wrapper
/// calling it, for one exported free function, class method, or
/// constructor. `receiver` is `Some(dart expr for the JS receiver object)`
/// for instance methods; constructors and free functions pass `None`.
fn render_exported_call(
    shape: &ExportedShape,
    js_module: &str,
    receiver: Option<&str>,
    context: &RenderContext<Wasm32>,
) -> Result<(String, String)> {
    let call_args = shape
        .params
        .iter()
        .map(|(name, ty)| to_js(name, ty, context))
        .collect::<Result<Vec<_>>>()?
        .join(", ");

    // Static functions/methods/constructors are reachable by module-qualified
    // path via a plain `@JS()` external declaration. Instance methods are
    // real JS object methods, not free functions taking the receiver as an
    // extra argument, so those go through dynamic property dispatch instead
    // (`dart:js_interop_unsafe`'s `callMethod`) -- avoids needing a
    // hand-written `extension type` per exported class.
    let (external_decl, call) = match receiver {
        None => {
            let external_name = format!("_js_{}", shape.js_name.replace('.', "_"));
            let js_param_list = shape
                .params
                .iter()
                .enumerate()
                .map(|(index, _)| format!("$$js.JSAny? __boltffiArg{index}"))
                .collect::<Vec<_>>()
                .join(", ");
            let js_return_ty = if shape.asynchronous {
                "$$js.JSPromise<$$js.JSAny?>"
            } else {
                "$$js.JSAny?"
            };
            let decl = format!(
                "@$$js.JS('{js_module}.{}')\nexternal {js_return_ty} {external_name}({js_param_list});",
                shape.js_name
            );
            (decl, format!("{external_name}({call_args})"))
        }
        Some(receiver) => {
            let method_args = if call_args.is_empty() {
                receiver.to_owned()
            } else {
                format!("{receiver}, {call_args}")
            };
            let cast_ty = if shape.asynchronous {
                "$$js.JSPromise<$$js.JSAny?>"
            } else {
                "$$js.JSAny?"
            };
            let method_name = shape.js_name.rsplit('.').next().unwrap_or(&shape.js_name);
            let call = format!(
                "({receiver}.getProperty('{method_name}'.toJS) as $$js.JSFunction).callAsFunction({method_args}) as {cast_ty}"
            );
            (String::new(), call)
        }
    };

    let decode_success = |value_expr: &str| -> Result<String> {
        match &shape.success_ty {
            None => Ok("null".to_owned()),
            Some(ty) => from_js(value_expr, ty, context),
        }
    };

    let public_return_ty = {
        let value_ty = match &shape.success_ty {
            Some(ty) => dart_type(ty, context)?,
            None => "void".to_owned(),
        };
        if shape.asynchronous {
            format!("$$asyncutil.CancelableOperation<{value_ty}>")
        } else {
            value_ty
        }
    };

    let body = if shape.asynchronous {
        let decode_error = match &shape.error_ty {
            Some(ty) => format!(
                "(__boltffiError) => {}",
                from_js("__boltffiError", ty, context)?
            ),
            None => "(__boltffiError) => StateError(__boltffiError.toString())".to_owned(),
        };
        let decode = decode_success("__boltffiValue")?;
        format!(
            "return $$asCancelable($$awaitFallible({call}, (__boltffiValue) => {decode}, {decode_error}));"
        )
    } else if shape.error_ty.is_some() {
        // The compiled TS module's fallible sync functions/methods don't
        // return a `{tag, value}` wrapper -- they return the plain success
        // value directly and *throw* a real typed `XxxException` (with a
        // `.value` holding the decoded error payload) on failure, matching
        // ordinary JS/TS exception idioms (see
        // `target::typescript::render::function`'s `Failure::render`). Any
        // wire-tag matching here would silently misread every successful
        // call (there is no tag to find).
        let decode = decode_success("__boltffiValue")?;
        let decode_error = from_js(
            "(__boltffiError.getProperty('value'.toJS) as $$js.JSAny)",
            shape.error_ty.as_ref().expect("checked above"),
            context,
        )?;
        format!(
            "try {{\n      final __boltffiValue = {call};\n      return {decode};\n    }} catch (__boltffiError) {{\n      if (__boltffiError is $$js.JSObject && __boltffiError.hasProperty('value'.toJS).toDart) {{\n        throw {decode_error};\n      }}\n      rethrow;\n    }}"
        )
    } else {
        match &shape.success_ty {
            None => format!("{call};"),
            Some(_) => {
                let decode = decode_success(&format!("({call})"))?;
                format!("return {decode};")
            }
        }
    };

    let params_sig = shape
        .params
        .iter()
        .map(|(name, ty)| Ok(format!("{} {name}", dart_type(ty, context)?)))
        .collect::<Result<Vec<_>>>()?
        .join(", ");

    Ok((
        external_decl,
        format!(
            "{public_return_ty} {}({params_sig}) {{\n    {body}\n  }}",
            shape.dart_name
        ),
    ))
}

pub fn function(
    decl: &FunctionDecl<Wasm32>,
    js_module: &str,
    context: &RenderContext<Wasm32>,
) -> Result<Emitted> {
    let dart_name = name_style::lower_camel(decl.name());
    let shape = exported_callable_shape(dart_name, js_name(decl.name()), decl.callable(), context)?;
    let (external_decl, wrapper) = render_exported_call(&shape, js_module, None, context)?;
    Ok(Emitted::primary(format!("{external_decl}\n{wrapper}\n\n")))
}

pub fn class(
    decl: &ClassDecl<Wasm32>,
    js_module: &str,
    context: &RenderContext<Wasm32>,
) -> Result<Emitted> {
    let name = name_style::upper_camel(decl.name());
    let mut externals = Vec::new();
    let mut initializers = Vec::new();
    for initializer in decl.initializers() {
        let dart_name = name_style::lower_camel(initializer.name());
        let member_js_name = format!("{}.{}", name, js_name(initializer.name()));
        let shape = exported_callable_shape(
            dart_name.clone(),
            member_js_name,
            initializer.callable(),
            context,
        )?;
        let (external_decl, wrapper) = render_exported_call(&shape, js_module, None, context)?;
        externals.push(external_decl);
        // Compare the raw (unescaped) name -- `dart_name` has already been
        // keyword-escaped to `new_` by this point, so it would never match
        // "new" here, always producing an unwanted `ClassName.new_(...)`
        // named constructor even for a plain Rust `fn new(...) -> Self`.
        let ctor_name = if js_name(initializer.name()) == "new" {
            name.clone()
        } else {
            format!("{name}.{dart_name}")
        };
        // `render_exported_call` renders a plain function; splice it into a
        // factory constructor by reusing its body/signature textually.
        let body_start = wrapper
            .find("{{")
            .unwrap_or_else(|| wrapper.find('{').unwrap_or(0));
        let params_start = wrapper.find('(').unwrap_or(0);
        let params_end = wrapper[params_start..]
            .find(')')
            .map(|end| params_start + end + 1)
            .unwrap_or(params_start);
        let params_sig = &wrapper[params_start..params_end];
        let body = &wrapper[body_start..];
        initializers.push(format!("  factory {ctor_name}{params_sig} {body}"));
    }
    let mut methods = Vec::new();
    for method in decl.methods() {
        let dart_name = name_style::lower_camel(method.name());
        let member_js_name = format!("{}.{}", name, js_name(method.name()));
        let static_prefix = if method.callable().receiver().is_none() {
            "static "
        } else {
            ""
        };
        let shape = exported_callable_shape(dart_name, member_js_name, method.callable(), context)?;
        let receiver = method.callable().receiver().map(|_| "this._js");
        let (external_decl, wrapper) = render_exported_call(&shape, js_module, receiver, context)?;
        externals.push(external_decl);
        methods.push(format!("  {static_prefix}{wrapper}"));
    }
    Ok(Emitted::primary(format!(
        "{}\n\nfinal class {name} {{\n  final $$js.JSObject _js;\n\n  {name}._fromJS(this._js);\n\n  $$js.JSObject _toJS() => _js;\n\n{}\n\n{}\n}}\n\n",
        externals.join("\n"),
        initializers.join("\n\n"),
        methods.join("\n\n")
    )))
}

#[allow(dead_code)]
fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn missing(kind: &'static str) -> Error {
    Error::BrokenBridgeContract {
        bridge: "dart_web",
        invariant: kind,
    }
}

fn unsupported<T>(shape: &'static str) -> Result<T> {
    Err(Error::UnsupportedTarget {
        target: "dart_web",
        shape,
    })
}

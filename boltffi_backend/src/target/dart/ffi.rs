use crate::{
    bridge::c::{CBridgeContract, Function, Record, Type},
    core::Result,
};

pub fn render(bridge: &CBridgeContract) -> Result<String> {
    let records = bridge
        .direct_records()
        .iter()
        .map(render_record)
        .chain(
            bridge
                .callbacks()
                .iter()
                .map(|callback| render_record(callback.vtable())),
        )
        .collect::<Vec<_>>()
        .join("\n\n");
    let functions = bridge
        .support()
        .functions()
        .iter()
        .chain(bridge.functions())
        .chain(
            bridge
                .callbacks()
                .iter()
                .flat_map(|callback| [callback.register(), callback.create_handle()]),
        )
        .map(render_function)
        .collect::<Vec<_>>()
        .join("\n\n");
    Ok(format!("{records}\n\n{functions}\n"))
}

pub fn record_name(record: &Record) -> String {
    named(record.name())
}

fn render_record(record: &Record) -> String {
    let fields = record
        .fields()
        .iter()
        .map(|field| {
            let annotation = annotation(field.ty());
            format!(
                "  {annotation}external {} {};",
                dart_type(field.ty()),
                field_name(field.name())
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!(
        "final class {} extends $$ffi.Struct {{\n{fields}\n}}",
        record_name(record)
    )
}

/// Dart-side name for a C struct/vtable field (escapes Dart keywords).
pub fn field_name(name: &str) -> String {
    super::name_style::escape_identifier(name)
}

fn render_function(function: &Function) -> String {
    let native_params = function
        .params()
        .iter()
        .map(|param| native_type(param.ty()))
        .collect::<Vec<_>>()
        .join(", ");
    let parameters = function
        .params()
        .iter()
        .map(|param| format!("  {} {},", dart_type(param.ty()), field_name(param.name())))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "@$$ffi.Native<{} Function({native_params})>(symbol: '{}')\nexternal {} _f${}(\n{parameters}\n);",
        native_type(function.returns()),
        function.name(),
        dart_type(function.returns()),
        function.name()
    )
}

pub fn native_type(ty: &Type) -> String {
    #[allow(unreachable_patterns)]
    match ty {
        Type::Void => "$$ffi.Void".into(),
        Type::Bool => "$$ffi.Bool".into(),
        Type::Int8 => "$$ffi.Int8".into(),
        Type::Uint8 => "$$ffi.Uint8".into(),
        Type::Int16 => "$$ffi.Int16".into(),
        Type::Uint16 => "$$ffi.Uint16".into(),
        Type::Int32 => "$$ffi.Int32".into(),
        Type::Uint32 => "$$ffi.Uint32".into(),
        Type::Int64 => "$$ffi.Int64".into(),
        Type::Uint64 => "$$ffi.Uint64".into(),
        Type::Float32 => "$$ffi.Float".into(),
        Type::Float64 => "$$ffi.Double".into(),
        Type::SignedPointerWidth => "$$ffi.IntPtr".into(),
        Type::PointerWidth => "$$ffi.UintPtr".into(),
        Type::Status => "_$$FFIStatus".into(),
        Type::Buffer => "_$$FFIBuf".into(),
        Type::String => "_$$FFIString".into(),
        Type::Span => "_$$FFISpan".into(),
        Type::FutureHandle => "$$ffi.Pointer<$$ffi.Void>".into(),
        Type::StreamPollResult | Type::WaitResult => "$$ffi.Int8".into(),
        Type::CallbackHandle(_) => "_$$BoltFFICallbackHandle".into(),
        Type::Named(name) | Type::DirectRecord(name) => named(name.as_str()),
        Type::CStyleEnum { repr, .. } => native_type(repr),
        Type::ConstPointer(inner) | Type::MutPointer(inner) => {
            format!("$$ffi.Pointer<{}>", native_type(inner))
        }
        Type::FunctionPointer { returns, params } => format!(
            "$$ffi.Pointer<$$ffi.NativeFunction<{} Function({})>>",
            native_type(returns),
            params
                .iter()
                .map(native_type)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => "$$ffi.Void".into(),
    }
}

pub fn dart_type(ty: &Type) -> String {
    match ty {
        Type::Void => "void".into(),
        Type::Bool => "bool".into(),
        Type::Int8
        | Type::Uint8
        | Type::Int16
        | Type::Uint16
        | Type::Int32
        | Type::Uint32
        | Type::Int64
        | Type::Uint64
        | Type::SignedPointerWidth
        | Type::PointerWidth
        | Type::StreamPollResult
        | Type::WaitResult => "int".into(),
        Type::Float32 | Type::Float64 => "double".into(),
        Type::CStyleEnum { repr, .. } => dart_type(repr),
        other => native_type(other),
    }
}

fn annotation(ty: &Type) -> String {
    match ty {
        Type::Bool => "@$$ffi.Bool()\n  ".into(),
        Type::Int8 => "@$$ffi.Int8()\n  ".into(),
        Type::Uint8 => "@$$ffi.Uint8()\n  ".into(),
        Type::Int16 => "@$$ffi.Int16()\n  ".into(),
        Type::Uint16 => "@$$ffi.Uint16()\n  ".into(),
        Type::Int32 => "@$$ffi.Int32()\n  ".into(),
        Type::Uint32 => "@$$ffi.Uint32()\n  ".into(),
        Type::Int64 => "@$$ffi.Int64()\n  ".into(),
        Type::Uint64 => "@$$ffi.Uint64()\n  ".into(),
        Type::Float32 => "@$$ffi.Float()\n  ".into(),
        Type::Float64 => "@$$ffi.Double()\n  ".into(),
        Type::SignedPointerWidth => "@$$ffi.IntPtr()\n  ".into(),
        Type::PointerWidth => "@$$ffi.UintPtr()\n  ".into(),
        _ => String::new(),
    }
}

fn named(value: &str) -> String {
    let value = value
        .trim_start_matches('_')
        .replace(|character: char| !character.is_ascii_alphanumeric(), "_");
    format!("_C${value}")
}

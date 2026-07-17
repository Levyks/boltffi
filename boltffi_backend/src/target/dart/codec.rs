use boltffi_binding::{
    BinderId, BuiltinType, CallbackId, ClassId, CodecRead, CodecWrite, CustomTypeId, ElementCount,
    EnumId, FieldKey, MapKind, Native, Op, Primitive, RecordId, ValueRef, ValueRoot,
};

use crate::core::{CustomTypeConversion, CustomTypeMapping, Error, RenderContext, Result};

use super::{DartHost, name_style, primitive};

pub struct Reader<'a, 'bindings> {
    name: &'a str,
    context: &'a RenderContext<'bindings, Native>,
}

pub struct Writer<'a, 'bindings> {
    name: &'a str,
    scope: String,
    context: &'a RenderContext<'bindings, Native>,
}

impl<'a, 'bindings> Reader<'a, 'bindings> {
    pub fn new(name: &'a str, context: &'a RenderContext<'bindings, Native>) -> Self {
        Self { name, context }
    }

    fn read(&self, method: &str) -> String {
        format!("{}.{}()", self.name, method)
    }
}

impl CodecRead for Reader<'_, '_> {
    type Expr = Result<String>;

    fn primitive(&mut self, primitive: Primitive) -> Self::Expr {
        Ok(self.read(&format!("read{}", primitive::wire_suffix(primitive)?)))
    }
    fn string(&mut self) -> Self::Expr {
        Ok(self.read("readString"))
    }
    fn interned_string(&mut self, values: &[String]) -> Self::Expr {
        let values = dart_string_list(values);
        Ok(format!("{}.readInternedString(const {values})", self.name))
    }
    fn bytes(&mut self) -> Self::Expr {
        Ok(self.read("readUint8List"))
    }
    fn direct_record(&mut self, id: RecordId) -> Self::Expr {
        self.record(id)
    }
    fn encoded_record(&mut self, id: RecordId) -> Self::Expr {
        self.record(id)
    }
    fn c_style_enum(&mut self, id: EnumId) -> Self::Expr {
        Ok(format!(
            "{}._fromValue({}.readI32())",
            type_name_enum(id, self.context)?,
            self.name
        ))
    }
    fn data_enum(&mut self, id: EnumId) -> Self::Expr {
        Ok(format!(
            "{}._decode({})",
            type_name_enum(id, self.context)?,
            self.name
        ))
    }
    fn class_handle(&mut self, _: ClassId) -> Self::Expr {
        unsupported("class handle codec read")
    }
    fn callback_handle(&mut self, _: CallbackId) -> Self::Expr {
        unsupported("callback handle codec read")
    }
    fn custom(&mut self, id: CustomTypeId, representation: Self::Expr) -> Self::Expr {
        let representation = representation?;
        match self.context.custom_type_mapping(id) {
            Some(mapping) => Ok(custom_type_decode(mapping, &representation)),
            None => Ok(representation),
        }
    }
    fn builtin(&mut self, kind: BuiltinType) -> Self::Expr {
        Ok(self.read(match kind {
            BuiltinType::Duration => "readDuration",
            BuiltinType::SystemTime => "readInstant",
            BuiltinType::Uuid => "readUuid",
            BuiltinType::Url => "readString",
        }))
    }
    fn optional(&mut self, inner: Self::Expr) -> Self::Expr {
        Ok(format!(
            "{}.readOptional(({}) => {})",
            self.name, self.name, inner?
        ))
    }
    fn sequence(&mut self, _: &Op<ElementCount>, element: Self::Expr) -> Self::Expr {
        Ok(format!(
            "{}.readList(({}) => {})",
            self.name, self.name, element?
        ))
    }
    fn tuple(&mut self, elements: Vec<Self::Expr>) -> Self::Expr {
        Ok(format!("({})", collect(elements)?.join(", ")))
    }
    fn result(&mut self, ok: Self::Expr, err: Self::Expr) -> Self::Expr {
        Ok(format!(
            "{}.readResult(({}) => {}, ({}) => {})",
            self.name, self.name, ok?, self.name, err?
        ))
    }
    fn map(&mut self, _: MapKind, key: Self::Expr, value: Self::Expr) -> Self::Expr {
        Ok(format!(
            "{}.readMap(({}) => {}, ({}) => {})",
            self.name, self.name, key?, self.name, value?
        ))
    }
}

impl Reader<'_, '_> {
    fn record(&self, id: RecordId) -> Result<String> {
        let name = self
            .context
            .record(id)
            .ok_or(Error::BrokenBridgeContract {
                bridge: DartHost::TARGET,
                invariant: "missing record in Dart codec reader",
            })
            .map(|decl| name_style::upper_camel(decl.name()))?;
        Ok(format!("{name}._decode({})", self.name))
    }
}

impl<'a, 'bindings> Writer<'a, 'bindings> {
    pub fn new(
        name: &'a str,
        scope: impl Into<String>,
        context: &'a RenderContext<'bindings, Native>,
    ) -> Self {
        Self {
            name,
            scope: scope.into(),
            context,
        }
    }

    fn value(&self, value: &ValueRef) -> Result<String> {
        render_value(value, &self.scope)
    }
    fn write(&self, method: &str, value: &ValueRef) -> Result<String> {
        Ok(format!("{}.{}({});", self.name, method, self.value(value)?))
    }

    fn with_scope(
        &mut self,
        scope: String,
        render: impl FnOnce(&mut Self, &ValueRef) -> Vec<Result<String>>,
    ) -> Vec<Result<String>> {
        let previous = std::mem::replace(&mut self.scope, scope);
        let statements = render(self, &ValueRef::self_value());
        self.scope = previous;
        statements
    }
}

impl CodecWrite for Writer<'_, '_> {
    type Stmt = Result<String>;

    fn primitive(&mut self, primitive: Primitive, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.write(
            &format!(
                "write{}",
                primitive::wire_suffix(primitive).unwrap_or("Unknown")
            ),
            value,
        )]
    }
    fn string(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.write("writeString", value)]
    }
    fn interned_string(&mut self, values: &[String], value: &ValueRef) -> Vec<Self::Stmt> {
        let values = dart_string_list(values);
        vec![self.value(value).map(|value| {
            format!(
                "{}.writeInternedString({value}, const {values});",
                self.name
            )
        })]
    }
    fn bytes(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.value(value).map(|value| {
            format!(
                "{}.writeU32({value}.length); {}.writeTypedList({value});",
                self.name, self.name
            )
        })]
    }
    fn direct_record(&mut self, _: RecordId, value: &ValueRef) -> Vec<Self::Stmt> {
        self.encodable(value)
    }
    fn encoded_record(&mut self, _: RecordId, value: &ValueRef) -> Vec<Self::Stmt> {
        self.encodable(value)
    }
    fn c_style_enum(&mut self, _: EnumId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![
            self.value(value)
                .map(|value| format!("{}.writeI32({value}.value);", self.name)),
        ]
    }
    fn data_enum(&mut self, _: EnumId, value: &ValueRef) -> Vec<Self::Stmt> {
        self.encodable(value)
    }
    fn class_handle(&mut self, _: ClassId, _: &ValueRef) -> Vec<Self::Stmt> {
        vec![unsupported("class handle codec write")]
    }
    fn callback_handle(&mut self, _: CallbackId, _: &ValueRef) -> Vec<Self::Stmt> {
        vec![unsupported("callback handle codec write")]
    }
    fn custom<F>(
        &mut self,
        id: CustomTypeId,
        value: &ValueRef,
        representation: F,
    ) -> Vec<Self::Stmt>
    where
        F: FnOnce(&mut Self, &ValueRef) -> Vec<Self::Stmt>,
    {
        match self.context.custom_type_mapping(id) {
            Some(mapping) => match self.value(value) {
                Ok(value) => self.with_scope(custom_type_encode(mapping, &value), representation),
                Err(error) => vec![Err(error)],
            },
            None => representation(self, value),
        }
    }
    fn builtin(&mut self, kind: BuiltinType, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.write(
            match kind {
                BuiltinType::Duration => "writeDuration",
                BuiltinType::SystemTime => "writeInstant",
                BuiltinType::Uuid => "writeUuid",
                BuiltinType::Url => "writeString",
            },
            value,
        )]
    }
    fn optional(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        inner: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![container(self, "writeOptional", value, &[binder], inner)]
    }
    fn sequence(
        &mut self,
        value: &ValueRef,
        _: &Op<ElementCount>,
        binder: BinderId,
        element: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![container(self, "writeList", value, &[binder], element)]
    }
    fn tuple(&mut self, _: &ValueRef, elements: Vec<Vec<Self::Stmt>>) -> Vec<Self::Stmt> {
        elements.into_iter().flatten().collect()
    }
    fn result(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        ok: Vec<Self::Stmt>,
        err: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        let result = (|| {
            Ok(format!(
                "{}.writeResult({}, ({}, {}) {{ {} }}, ({}, {}) {{ {} }});",
                self.name,
                self.value(value)?,
                name_style::binder(binder.raw()),
                self.name,
                join(ok)?,
                name_style::binder(binder.raw()),
                self.name,
                join(err)?
            ))
        })();
        vec![result]
    }
    fn map(
        &mut self,
        _: MapKind,
        value: &ValueRef,
        key_binder: BinderId,
        key: Vec<Self::Stmt>,
        value_binder: BinderId,
        map_value: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        let result = (|| {
            Ok(format!(
                "{}.writeMap({}, ({}, {}) {{ {} }}, ({}, {}) {{ {} }});",
                self.name,
                self.value(value)?,
                name_style::binder(key_binder.raw()),
                self.name,
                join(key)?,
                name_style::binder(value_binder.raw()),
                self.name,
                join(map_value)?
            ))
        })();
        vec![result]
    }
}

impl Writer<'_, '_> {
    fn encodable(&self, value: &ValueRef) -> Vec<Result<String>> {
        vec![
            self.value(value)
                .map(|value| format!("{value}._encode({});", self.name)),
        ]
    }
}

fn container(
    writer: &Writer<'_, '_>,
    method: &str,
    value: &ValueRef,
    binders: &[BinderId],
    body: Vec<Result<String>>,
) -> Result<String> {
    Ok(format!(
        "{}.{}({}, ({}, {}) {{ {} }});",
        writer.name,
        method,
        writer.value(value)?,
        name_style::binder(binders[0].raw()),
        writer.name,
        join(body)?
    ))
}

fn render_value(value: &ValueRef, self_value: &str) -> Result<String> {
    let root = match value.root() {
        ValueRoot::SelfValue => self_value.to_owned(),
        ValueRoot::Named(name) | ValueRoot::Local(name) => name_style::lower_camel(name),
        ValueRoot::Binder(id) => name_style::binder(id.raw()),
        _ => return unsupported("unknown codec value root"),
    };
    Ok(value.path().iter().fold(root, |base, field| {
        // Bare field access (empty `base`) means we're referencing the
        // enclosing class's own declared property inside its instance
        // method (e.g. `field0`, set by `name_style::field`), not Dart's
        // native record positional accessor (`.$1`), which only applies
        // to actual `TypeRef::Tuple` values reached through a non-empty base.
        let bare = base.is_empty();
        let segment = match field {
            FieldKey::Named(name) => name_style::lower_camel(name),
            FieldKey::Position(position) if bare => format!("field{position}"),
            FieldKey::Position(position) => format!("${}", position + 1),
            _ => unreachable!("unknown field key"),
        };
        // Prefer bare field access inside instance methods (`message`) over
        // `this.message`, which triggers dart analyze's unnecessary_this lint.
        if bare {
            segment
        } else {
            format!("{base}.{segment}")
        }
    }))
}

fn type_name_enum(id: EnumId, context: &RenderContext<Native>) -> Result<String> {
    context
        .enumeration(id)
        .map(|decl| name_style::upper_camel(decl.name()))
        .ok_or(Error::BrokenBridgeContract {
            bridge: DartHost::TARGET,
            invariant: "missing enum in Dart codec",
        })
}

fn collect(values: Vec<Result<String>>) -> Result<Vec<String>> {
    values.into_iter().collect()
}
fn dart_string_list(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| format!(
                "'{}'",
                value
                    .replace('\\', "\\\\")
                    .replace('\'', "\\'")
                    .replace('$', "\\$")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t")
            ))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
fn join(values: Vec<Result<String>>) -> Result<String> {
    Ok(collect(values)?.join(" "))
}
fn unsupported<T>(shape: &'static str) -> Result<T> {
    Err(Error::UnsupportedTarget {
        target: DartHost::TARGET,
        shape,
    })
}

/// Converts a wire representation into the configured Dart target type.
fn custom_type_decode(mapping: &CustomTypeMapping, representation: &str) -> String {
    match mapping.conversion() {
        CustomTypeConversion::UuidString | CustomTypeConversion::UrlString => {
            match mapping.target_type().as_str() {
                "String" => representation.to_owned(),
                "Uri" => format!("Uri.parse({representation})"),
                target => format!("{target}.parse({representation})"),
            }
        }
    }
}

/// Converts a configured Dart target value into the wire representation.
fn custom_type_encode(mapping: &CustomTypeMapping, value: &str) -> String {
    match mapping.conversion() {
        CustomTypeConversion::UuidString | CustomTypeConversion::UrlString => {
            match mapping.target_type().as_str() {
                "String" => value.to_owned(),
                _ => format!("{value}.toString()"),
            }
        }
    }
}

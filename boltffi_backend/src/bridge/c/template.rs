use askama::Template as AskamaTemplate;

use crate::core::Result;

use super::contract::{CBridgeContract, Callback, Enum, Field, Function, Record};
use super::identifier::Identifier;
use super::syntax::{FunctionSyntax, Statement, TypeFragment, TypeSyntax};

#[derive(AskamaTemplate)]
#[template(path = "bridge/c/header.h", escape = "none")]
struct HeaderTemplate {
    support_functions: Vec<FunctionView>,
    direct_records: Vec<RecordView>,
    enums: Vec<EnumView>,
    callback_vtables: Vec<RecordView>,
    callback_functions: Vec<FunctionView>,
    functions: Vec<FunctionView>,
}

struct RecordView {
    name: Identifier,
    fields: Vec<FieldView>,
}

struct FieldView {
    declaration: Statement,
}

struct EnumView {
    name: Identifier,
    repr: TypeFragment,
    variants: Vec<EnumVariantView>,
}

struct EnumVariantView {
    name: Identifier,
    ty: Identifier,
    value: i128,
}

struct FunctionView {
    declaration: Statement,
}

pub struct Header<'abi> {
    abi: &'abi CBridgeContract,
}

impl<'abi> Header<'abi> {
    pub fn new(abi: &'abi CBridgeContract) -> Self {
        Self { abi }
    }

    pub fn render(self) -> Result<String> {
        Ok(HeaderTemplate {
            support_functions: self
                .abi
                .support()
                .functions()
                .iter()
                .map(FunctionView::from_function)
                .collect::<Result<_>>()?,
            direct_records: self
                .abi
                .direct_records()
                .iter()
                .map(RecordView::from_record)
                .collect::<Result<_>>()?,
            enums: self
                .abi
                .enums()
                .iter()
                .map(EnumView::from_enum)
                .collect::<Result<_>>()?,
            callback_vtables: self
                .abi
                .callbacks()
                .iter()
                .map(Callback::vtable)
                .map(RecordView::from_record)
                .collect::<Result<_>>()?,
            callback_functions: self
                .abi
                .callbacks()
                .iter()
                .flat_map(|callback| [callback.register(), callback.create_handle()])
                .map(FunctionView::from_function)
                .collect::<Result<_>>()?,
            functions: self
                .abi
                .functions()
                .iter()
                .map(FunctionView::from_function)
                .collect::<Result<_>>()?,
        }
        .render()?)
    }
}

impl RecordView {
    fn from_record(record: &Record) -> Result<Self> {
        Ok(Self {
            name: Identifier::parse(record.name())?,
            fields: record
                .fields()
                .iter()
                .map(FieldView::from_field)
                .collect::<Result<_>>()?,
        })
    }
}

impl FieldView {
    fn from_field(field: &Field) -> Result<Self> {
        Ok(Self {
            declaration: TypeSyntax::new(field.ty()).declaration(field.name())?,
        })
    }
}

impl EnumView {
    fn from_enum(enumeration: &Enum) -> Result<Self> {
        Ok(Self {
            name: Identifier::parse(enumeration.name())?,
            repr: TypeSyntax::new(enumeration.repr()).anonymous()?,
            variants: enumeration
                .variants()
                .iter()
                .map(|variant| {
                    Ok(EnumVariantView {
                        name: Identifier::parse(variant.name())?,
                        ty: Identifier::parse(enumeration.name())?,
                        value: variant.value(),
                    })
                })
                .collect::<Result<_>>()?,
        })
    }
}

impl FunctionView {
    fn from_function(function: &Function) -> Result<Self> {
        Ok(Self {
            declaration: FunctionSyntax::new(function).declaration()?,
        })
    }
}

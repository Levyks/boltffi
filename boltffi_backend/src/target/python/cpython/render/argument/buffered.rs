use crate::{
    core::{Error, Result},
    target::python::cpython::render::{direct_vector, primitive},
};

#[derive(Clone)]
pub enum BufferedArgument {
    OptionalPrimitive(primitive::Runtime),
    RegisteredObject(RegisteredObject),
    RawWire,
    DirectVector(direct_vector::Element),
}

impl BufferedArgument {
    pub fn parser(&self) -> Result<String> {
        match self {
            Self::OptionalPrimitive(primitive) => primitive.optional_wire_encoder(),
            Self::RegisteredObject(registered) => Ok(registered.parser.clone()),
            Self::RawWire => Ok("boltffi_python_wire_raw".to_owned()),
            Self::DirectVector(element) => Ok(element.vector_parser().to_owned()),
        }
    }

    pub fn call_args(
        &self,
        pointer: &str,
        length: &str,
        mutation: Option<&MutationOutput>,
    ) -> Vec<String> {
        match self {
            Self::DirectVector(element) => vec![
                format!("(const {} *){pointer}", element.c_type()),
                length.to_owned(),
            ],
            Self::OptionalPrimitive(_) | Self::RegisteredObject(_) | Self::RawWire => {
                [pointer.to_owned(), length.to_owned()]
                    .into_iter()
                    .chain(mutation.map(|mutation| format!("&{}", mutation.buffer())))
                    .collect()
            }
        }
    }

    pub fn mutation_output(&self, name: &str) -> Result<Option<MutationOutput>> {
        match self {
            Self::RegisteredObject(registered) => Ok(Some(MutationOutput::new(
                format!("{name}_out"),
                registered.owned_decoder.clone(),
            ))),
            Self::OptionalPrimitive(_) | Self::RawWire | Self::DirectVector(_) => {
                Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "mutable encoded parameter",
                })
            }
        }
    }

    pub fn primitive(&self) -> Option<primitive::Runtime> {
        match self {
            Self::OptionalPrimitive(primitive) => Some(*primitive),
            Self::RegisteredObject(_) | Self::RawWire | Self::DirectVector(_) => None,
        }
    }

    pub fn direct_vector_element(&self) -> Option<direct_vector::Element> {
        match self {
            Self::DirectVector(element) => Some(element.clone()),
            Self::OptionalPrimitive(_) | Self::RegisteredObject(_) | Self::RawWire => None,
        }
    }

    pub fn is_raw_wire(&self) -> bool {
        matches!(self, Self::RawWire)
    }
}

#[derive(Clone)]
pub struct RegisteredObject {
    parser: String,
    owned_decoder: String,
}

impl RegisteredObject {
    pub fn new(parser: impl Into<String>, owned_decoder: impl Into<String>) -> Self {
        Self {
            parser: parser.into(),
            owned_decoder: owned_decoder.into(),
        }
    }
}

#[derive(Clone)]
pub struct MutationOutput {
    buffer: String,
    decoder: String,
}

impl MutationOutput {
    fn new(buffer: impl Into<String>, decoder: impl Into<String>) -> Self {
        Self {
            buffer: buffer.into(),
            decoder: decoder.into(),
        }
    }

    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    pub fn decoder(&self) -> &str {
        &self.decoder
    }
}

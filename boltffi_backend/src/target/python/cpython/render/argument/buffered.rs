use crate::{
    bridge::c::{Expression, Identifier, TypeFragment},
    core::{Error, Result},
    target::python::cpython::render::{direct_vector, primitive, result},
};

#[derive(Clone)]
pub enum BufferedArgument {
    OptionalPrimitive(primitive::Runtime),
    RegisteredObject(RegisteredObject),
    RawWire,
    DirectVector(direct_vector::Element),
}

impl BufferedArgument {
    pub fn parser(&self) -> Result<Identifier> {
        match self {
            Self::OptionalPrimitive(primitive) => primitive.optional_wire_encoder(),
            Self::RegisteredObject(registered) => Ok(registered.parser.clone()),
            Self::RawWire => Identifier::parse("boltffi_python_wire_raw"),
            Self::DirectVector(element) => Ok(element.vector_parser().clone()),
        }
    }

    pub fn call_args(
        &self,
        pointer: &Identifier,
        length: &Identifier,
        mutation: Option<&MutationOutput>,
    ) -> Result<Vec<Expression>> {
        match self {
            Self::DirectVector(element) => Ok(vec![
                Expression::cast(
                    TypeFragment::new(format!("const {} *", element.c_type())),
                    Expression::identifier(pointer.clone()),
                ),
                Expression::identifier(length.clone()),
            ]),
            Self::OptionalPrimitive(_) | Self::RegisteredObject(_) | Self::RawWire => {
                Ok([pointer, length]
                    .into_iter()
                    .cloned()
                    .map(Expression::identifier)
                    .chain(
                        mutation
                            .map(MutationOutput::buffer)
                            .cloned()
                            .map(Expression::identifier)
                            .map(Expression::address_of),
                    )
                    .collect())
            }
        }
    }

    pub fn mutation_output(&self, name: &Identifier) -> Result<Option<MutationOutput>> {
        match self {
            Self::RegisteredObject(registered) => Ok(Some(MutationOutput::new(
                Identifier::parse(format!("{name}_out"))?,
                registered.owned_decoder.clone(),
                None,
            ))),
            Self::RawWire => Ok(Some(MutationOutput::new(
                Identifier::parse(format!("{name}_out"))?,
                result::OwnedBuffer::RawWire.converter()?,
                Some(result::OwnedBuffer::RawWire),
            ))),
            Self::OptionalPrimitive(_) | Self::DirectVector(_) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "mutable encoded parameter",
            }),
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
    parser: Identifier,
    owned_decoder: Identifier,
}

impl RegisteredObject {
    pub fn new(parser: Identifier, owned_decoder: Identifier) -> Self {
        Self {
            parser,
            owned_decoder,
        }
    }
}

#[derive(Clone)]
pub struct MutationOutput {
    buffer: Identifier,
    decoder: Identifier,
    owned_buffer: Option<result::OwnedBuffer>,
}

impl MutationOutput {
    fn new(
        buffer: Identifier,
        decoder: Identifier,
        owned_buffer: Option<result::OwnedBuffer>,
    ) -> Self {
        Self {
            buffer,
            decoder,
            owned_buffer,
        }
    }

    pub fn buffer(&self) -> &Identifier {
        &self.buffer
    }

    pub fn decoder(&self) -> &Identifier {
        &self.decoder
    }

    pub fn owned_buffer(&self) -> Option<result::OwnedBuffer> {
        self.owned_buffer.clone()
    }
}

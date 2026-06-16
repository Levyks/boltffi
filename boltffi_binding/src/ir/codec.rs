use boltffi_ast::BuiltinType;
use serde::{Deserialize, Serialize};

use crate::{
    CallbackId, ClassId, CustomTypeId, ElementCount, EnumId, Op, Primitive, RecordId, ValueRef,
};

/// Instructions for reconstructing one value from its boundary bytes.
///
/// A read plan is tree-shaped because an encoded value can contain other
/// encoded values: a `Vec<UserProfile>` is a sequence whose element is a
/// record whose fields are themselves encoded. The plan names the tree
/// once and every reader walks the same shape.
///
/// # Example
///
/// A `Vec<String>` is described by a [`CodecNode::Sequence`] whose element
/// is [`CodecNode::String`].
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ReadPlan {
    root: CodecNode,
}

impl ReadPlan {
    pub(crate) fn new(root: CodecNode) -> Self {
        Self { root }
    }

    /// Returns the root codec node.
    pub fn root(&self) -> &CodecNode {
        &self.root
    }
}

/// Instructions for emitting one value as boundary bytes.
///
/// Mirror of [`ReadPlan`] for the encode direction. The value reference
/// names which already-bound value the plan consumes, so generated code
/// does not have to invent a path expression by string convention.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct WritePlan {
    value: ValueRef,
    root: CodecNode,
}

impl WritePlan {
    pub(crate) fn new(value: ValueRef, root: CodecNode) -> Self {
        Self { value, root }
    }

    /// Returns the value the plan consumes.
    pub fn value(&self) -> &ValueRef {
        &self.value
    }

    /// Returns the root codec node.
    pub fn root(&self) -> &CodecNode {
        &self.root
    }
}

/// Bidirectional codec selected for one encoded value.
///
/// Encoded records and data enums always need both directions. Keeping the
/// pair together prevents construction sites from passing unrelated read and
/// write plans for the same declaration.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CodecPlan {
    read: ReadPlan,
    write: WritePlan,
}

impl CodecPlan {
    pub(crate) fn new(read: ReadPlan, write: WritePlan) -> Self {
        Self { read, write }
    }

    /// Returns the plan used to read the encoded value.
    pub fn read(&self) -> &ReadPlan {
        &self.read
    }

    /// Returns the plan used to write the encoded value.
    pub fn write(&self) -> &WritePlan {
        &self.write
    }
}

/// One node in a codec tree.
///
/// Names a value that requires encoding work to cross the boundary:
/// primitives, strings, byte buffers, optional and repeated values,
/// container shapes, and references to user-declared types. A node that
/// references a record knows whether the record crosses by direct memory
/// or by encoded payload, so a renderer that walks the tree cannot pick a
/// different boundary strategy for the nested value.
///
/// # Example
///
/// `Optional(Box<Sequence(Box<String>)>)` describes
/// `Option<Vec<String>>`. The same shape is what reading and writing
/// agree on; only the direction differs at render time.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CodecNode {
    /// Primitive scalar value.
    Primitive(Primitive),
    /// UTF-8 string value.
    String,
    /// Byte buffer value.
    Bytes,
    /// Record carried by direct memory layout.
    DirectRecord(RecordId),
    /// Record carried by encoded fields.
    EncodedRecord(RecordId),
    /// Fieldless enum carried by an integer discriminant.
    CStyleEnum(EnumId),
    /// Payload-carrying enum carried by a tag and payload.
    DataEnum(EnumId),
    /// Class instance carried by a handle.
    ClassHandle(ClassId),
    /// Callback object carried by a handle.
    CallbackHandle(CallbackId),
    /// Custom type carried through its selected representation.
    Custom(CustomTypeId),
    /// Builtin value carried through its BoltFFI wire representation.
    Builtin(BuiltinType),
    /// Optional value with a presence marker followed by the inner value.
    Optional(Box<CodecNode>),
    /// Repeated values prefixed by an element count.
    Sequence {
        /// Expression that yields the number of elements.
        len: Op<ElementCount>,
        /// Codec used for each element.
        element: Box<CodecNode>,
    },
    /// Fixed-size ordered group of values.
    Tuple(Vec<CodecNode>),
    /// Fallible value with success and error payload codecs.
    Result {
        /// Codec used for the success payload.
        ok: Box<CodecNode>,
        /// Codec used for the error payload.
        err: Box<CodecNode>,
    },
    /// Key-value collection.
    Map {
        /// Codec used for each key.
        key: Box<CodecNode>,
        /// Codec used for each value.
        value: Box<CodecNode>,
    },
}

//! Implementation of hashing values for the call cache.

use std::path::Path;
use std::path::PathBuf;

use blake3::Hasher;
use num_enum::IntoPrimitive;
use url::Url;
use wdl_analysis::types::Type;

use crate::Array;
use crate::CompoundValue;
use crate::HiddenValue;
use crate::HintsValue;
use crate::InputValue;
use crate::Map;
use crate::Object;
use crate::OutputValue;
use crate::Pair;
use crate::PrimitiveValue;
use crate::Struct;
use crate::Value;
use crate::digest::Digest;
use crate::path::EvaluationPath;

/// Represents the kind of hashed value.
#[derive(IntoPrimitive)]
#[repr(u8)]
enum ValueKind {
    /// The value is a `None`.
    None,
    /// The value is a `Boolean`.
    Boolean,
    /// The value is an `Int`.
    Integer,
    /// The value is a `Float`.
    Float,
    /// The value is a `String`.
    String,
    /// The value is a `File`
    File,
    /// The value is a `Directory`.
    Directory,
    /// The value is a `Pair`.
    Pair,
    /// The value is an `Array`.
    Array,
    /// The value is a `Map`.
    Map,
    /// The value is an `Object`.
    Object,
    /// The value is a `Struct`.
    Struct,
    /// The value is a `Hints` (hidden type).
    Hints,
    /// The value is an `Inputs` (hidden type).
    Inputs,
    /// The value is an `Outputs` (hidden type).
    Outputs,
}

/// Represents the kind of a content digest header.
#[derive(IntoPrimitive)]
#[repr(u8)]
enum ContentDigestKind {
    /// The content digest is from a hash algorithm.
    Hash,
    /// The content digest is from an ETag header.
    ETag,
}

/// Represents the kind of a digest.
#[derive(IntoPrimitive)]
#[repr(u8)]
enum DigestKind {
    /// The content digest is for a file.
    File,
    /// The content digest is for a directory.
    Directory,
}

/// Represents the kind of a hashed evaluation path.
#[derive(IntoPrimitive)]
#[repr(u8)]
enum PathKind {
    /// The path is to a local file.
    Local,
    /// The path is a URL to a remote file.
    Remote,
}

/// Hashes a sequence of hashable items.
pub fn hash_sequence<'a, T: Hashable + 'a>(
    hasher: &mut Hasher,
    items: impl ExactSizeIterator<Item = T>,
) {
    hasher.update(&(items.len() as u32).to_le_bytes());
    for item in items {
        item.hash(hasher);
    }
}

/// Trait used to implement WDL value hashing for call caching.
pub trait Hashable {
    /// Hashes into the given Blake3 hasher.
    fn hash(&self, hasher: &mut Hasher);
}

impl Hashable for &str {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(&(self.len() as u32).to_le_bytes());
        hasher.update(self.as_bytes());
    }
}

impl Hashable for String {
    fn hash(&self, hasher: &mut Hasher) {
        self.as_str().hash(hasher);
    }
}

impl Hashable for &[u8] {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(self);
    }
}

impl Hashable for Path {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(&(self.as_os_str().len() as u32).to_le_bytes());
        hasher.update(self.to_string_lossy().as_bytes());
    }
}

impl Hashable for PathBuf {
    fn hash(&self, hasher: &mut Hasher) {
        self.as_path().hash(hasher)
    }
}

impl Hashable for Url {
    fn hash(&self, hasher: &mut Hasher) {
        self.as_str().hash(hasher);
    }
}

impl Hashable for cloud_copy::ContentDigest {
    fn hash(&self, hasher: &mut Hasher) {
        match self {
            cloud_copy::ContentDigest::Hash { algorithm, digest } => {
                hasher.update(&[ContentDigestKind::Hash.into()]);
                algorithm.hash(hasher);
                digest.as_slice().hash(hasher);
            }
            cloud_copy::ContentDigest::ETag(etag) => {
                hasher.update(&[ContentDigestKind::ETag.into()]);
                etag.hash(hasher);
            }
        }
    }
}

impl Hashable for Digest {
    fn hash(&self, hasher: &mut Hasher) {
        match self {
            Digest::File(digest) => {
                hasher.update(&[DigestKind::File.into()]);
                digest.as_bytes().as_slice().hash(hasher);
            }
            Digest::Directory(digest) => {
                hasher.update(&[DigestKind::Directory.into()]);
                digest.as_bytes().as_slice().hash(hasher);
            }
        }
    }
}

impl Hashable for EvaluationPath {
    fn hash(&self, hasher: &mut Hasher) {
        match self {
            Self::Local(path) => {
                hasher.update(&[PathKind::Local.into()]);
                path.hash(hasher);
            }
            Self::Remote(url) => {
                hasher.update(&[PathKind::Remote.into()]);
                url.hash(hasher);
            }
        }
    }
}

impl Hashable for Option<PrimitiveValue> {
    fn hash(&self, hasher: &mut Hasher) {
        match self {
            Some(v) => v.hash(hasher),
            None => {
                // A `None` for an optional primitive value (used in map keys) represents a WDL
                // `None` value, so hash it as one
                Value::None(Type::None).hash(hasher)
            }
        }
    }
}

impl<K: Hashable, V: Hashable> Hashable for (K, V) {
    fn hash(&self, hasher: &mut Hasher) {
        self.0.hash(hasher);
        self.1.hash(hasher);
    }
}

impl<T: Hashable> Hashable for &T {
    fn hash(&self, hasher: &mut Hasher) {
        (*self).hash(hasher);
    }
}

impl Hashable for Value {
    fn hash(&self, hasher: &mut Hasher) {
        match self {
            Self::None(_) => {
                hasher.update(&[ValueKind::None.into()]);
            }
            Self::Primitive(v) => {
                v.hash(hasher);
            }
            Self::Compound(v) => {
                v.hash(hasher);
            }
            Self::Hidden(HiddenValue::Hints(v)) => {
                v.hash(hasher);
            }
            Self::Hidden(HiddenValue::Input(v)) => {
                v.hash(hasher);
            }
            Self::Hidden(HiddenValue::Output(v)) => v.hash(hasher),
            Self::Hidden(HiddenValue::TaskPreEvaluation(_))
            | Self::Hidden(HiddenValue::TaskPostEvaluation(_))
            | Self::Hidden(HiddenValue::PreviousTaskData(_))
            | Self::Call(_) => unreachable!("value cannot be hashed"),
        }
    }
}

impl Hashable for PrimitiveValue {
    fn hash(&self, hasher: &mut Hasher) {
        match self {
            Self::Boolean(v) => {
                hasher.update(&[ValueKind::Boolean.into()]);
                hasher.update(&[if *v { 1u8 } else { 0u8 }]);
            }
            Self::Integer(v) => {
                hasher.update(&[ValueKind::Integer.into()]);
                hasher.update(&v.to_le_bytes());
            }
            Self::Float(v) => {
                hasher.update(&[ValueKind::Float.into()]);
                hasher.update(&v.to_le_bytes());
            }
            Self::String(v) => {
                hasher.update(&[ValueKind::String.into()]);
                v.as_str().hash(hasher);
            }
            Self::File(v) => {
                hasher.update(&[ValueKind::File.into()]);
                v.as_str().hash(hasher);
            }
            Self::Directory(v) => {
                hasher.update(&[ValueKind::Directory.into()]);
                v.as_str().hash(hasher);
            }
        }
    }
}

impl Hashable for CompoundValue {
    fn hash(&self, hasher: &mut Hasher) {
        match self {
            Self::Pair(v) => v.hash(hasher),
            Self::Array(v) => v.hash(hasher),
            Self::Map(v) => v.hash(hasher),
            Self::Object(v) => v.hash(hasher),
            Self::Struct(v) => v.hash(hasher),
        }
    }
}

impl Hashable for Pair {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(&[ValueKind::Pair.into()]);
        self.left().hash(hasher);
        self.right().hash(hasher);
    }
}

impl Hashable for Array {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(&[ValueKind::Array.into()]);
        hash_sequence(hasher, self.as_slice().iter());
    }
}

impl Hashable for Map {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(&[ValueKind::Map.into()]);
        hash_sequence(hasher, self.iter());
    }
}

impl Hashable for Object {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(&[ValueKind::Object.into()]);
        hash_sequence(hasher, self.iter());
    }
}

impl Hashable for Struct {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(&[ValueKind::Struct.into()]);
        hash_sequence(hasher, self.iter());
    }
}

impl Hashable for HintsValue {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(&[ValueKind::Hints.into()]);
        hash_sequence(hasher, self.as_object().iter());
    }
}

impl Hashable for InputValue {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(&[ValueKind::Inputs.into()]);
        hash_sequence(hasher, self.as_object().iter());
    }
}

impl Hashable for OutputValue {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(&[ValueKind::Outputs.into()]);
        hash_sequence(hasher, self.as_object().iter());
    }
}

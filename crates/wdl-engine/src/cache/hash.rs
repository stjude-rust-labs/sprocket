//! Implementation of hashing values for the call cache.

use std::path::Path;
use std::path::PathBuf;

use blake3::Hasher;
use url::Url;

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

/// The variant tag for `None` values.
const NONE_VARIANT_TAG: u8 = 0;
/// The variant tag for `Boolean` values.
const BOOLEAN_VARIANT_TAG: u8 = 1;
/// The variant tag for `Int` values.
const INTEGER_VARIANT_TAG: u8 = 2;
/// The variant tag for `Float` values.
const FLOAT_VARIANT_TAG: u8 = 3;
/// The variant tag for `String` values.
const STRING_VARIANT_TAG: u8 = 4;
/// The variant tag for `File` values.
const FILE_VARIANT_TAG: u8 = 5;
/// The variant tag for `Directory` values.
const DIRECTORY_VARIANT_TAG: u8 = 6;
/// The variant tag for `Pair` values.
const PAIR_VARIANT_TAG: u8 = 7;
/// The variant tag for `Array` values.
const ARRAY_VARIANT_TAG: u8 = 8;
/// The variant tag for `Map` values.
const MAP_VARIANT_TAG: u8 = 9;
/// The variant tag for `Object` values.
const OBJECT_VARIANT_TAG: u8 = 10;
/// The variant tag for `Struct` values.
const STRUCT_VARIANT_TAG: u8 = 11;
/// The variant tag for `hints` values.
const HINTS_VARIANT_TAG: u8 = 12;
/// The variant tag for `input` values.
const INPUT_VARIANT_TAG: u8 = 13;
/// The variant tag for `output` values.
const OUTPUT_VARIANT_TAG: u8 = 14;

/// The variant tag for local evaluation paths.
const LOCAL_PATH_VARIANT_TAG: u8 = 0;
/// The variant tag for remote evaluation paths.
const REMOTE_PATH_VARIANT_TAG: u8 = 1;

/// The variant tag for hash-based remote content digests.
const CONTENT_HASH_VARIANT_TAG: u8 = 0;
/// The variant tag for ETag based remote content digests.
const CONTENT_ETAG_VARIANT_TAG: u8 = 1;

/// The variant tag for file digests.
const DIGEST_FILE_VARIANT_TAG: u8 = 0;
/// The variant tag for directory digests.
const DIGEST_DIRECTORY_VARIANT_TAG: u8 = 1;

/// Hashes a sequence of hashable items.
fn hash_sequence<'a, T: Hashable + 'a>(
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
        hasher.update(self.as_os_str().as_encoded_bytes());
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
                hasher.update(&[CONTENT_HASH_VARIANT_TAG]);
                algorithm.hash(hasher);
                digest.as_slice().hash(hasher);
            }
            cloud_copy::ContentDigest::ETag(etag) => {
                hasher.update(&[CONTENT_ETAG_VARIANT_TAG]);
                etag.hash(hasher);
            }
        }
    }
}

impl Hashable for Digest {
    fn hash(&self, hasher: &mut Hasher) {
        match self {
            Digest::File(digest) => {
                hasher.update(&[DIGEST_FILE_VARIANT_TAG]);
                digest.as_bytes().as_slice().hash(hasher);
            }
            Digest::Directory(digest) => {
                hasher.update(&[DIGEST_DIRECTORY_VARIANT_TAG]);
                digest.as_bytes().as_slice().hash(hasher);
            }
        }
    }
}

impl Hashable for EvaluationPath {
    fn hash(&self, hasher: &mut Hasher) {
        match self {
            Self::Local(path) => {
                hasher.update(&[LOCAL_PATH_VARIANT_TAG]);
                path.hash(hasher);
            }
            Self::Remote(url) => {
                hasher.update(&[REMOTE_PATH_VARIANT_TAG]);
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
                hasher.update(&[NONE_VARIANT_TAG]);
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
                hasher.update(&[NONE_VARIANT_TAG]);
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
                hasher.update(&[BOOLEAN_VARIANT_TAG]);
                hasher.update(&[if *v { 1u8 } else { 0u8 }]);
            }
            Self::Integer(v) => {
                hasher.update(&[INTEGER_VARIANT_TAG]);
                hasher.update(&v.to_le_bytes());
            }
            Self::Float(v) => {
                hasher.update(&[FLOAT_VARIANT_TAG]);
                hasher.update(&v.to_le_bytes());
            }
            Self::String(v) => {
                hasher.update(&[STRING_VARIANT_TAG]);
                v.as_str().hash(hasher);
            }
            Self::File(v) => {
                hasher.update(&[FILE_VARIANT_TAG]);
                v.as_str().hash(hasher);
            }
            Self::Directory(v) => {
                hasher.update(&[DIRECTORY_VARIANT_TAG]);
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
        hasher.update(&[PAIR_VARIANT_TAG]);
        self.left().hash(hasher);
        self.right().hash(hasher);
    }
}

impl Hashable for Array {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(&[ARRAY_VARIANT_TAG]);
        hash_sequence(hasher, self.as_slice().iter());
    }
}

impl Hashable for Map {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(&[MAP_VARIANT_TAG]);
        hash_sequence(hasher, self.iter());
    }
}

impl Hashable for Object {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(&[OBJECT_VARIANT_TAG]);
        hash_sequence(hasher, self.iter());
    }
}

impl Hashable for Struct {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(&[STRUCT_VARIANT_TAG]);
        hash_sequence(hasher, self.iter());
    }
}

impl Hashable for HintsValue {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(&[HINTS_VARIANT_TAG]);
        hash_sequence(hasher, self.as_object().iter());
    }
}

impl Hashable for InputValue {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(&[INPUT_VARIANT_TAG]);
        hash_sequence(hasher, self.as_object().iter());
    }
}

impl Hashable for OutputValue {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(&[OUTPUT_VARIANT_TAG]);
        hash_sequence(hasher, self.as_object().iter());
    }
}

//! Implementation of hashing values for the call cache.

use std::path::Path;
use std::path::PathBuf;

use blake3::Hasher;
use num_enum::IntoPrimitive;
use url::Url;
use wdl_analysis::types::Type;

use crate::Array;
use crate::CompoundValue;
use crate::ContentKind;
use crate::EnumVariant;
use crate::EvaluationPath;
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

/// Trait used to implement WDL value hashing for call caching.
pub trait Hashable {
    /// Hashes into the given Blake3 hasher.
    fn hash(&self, hasher: &mut Hasher);
}

/// Represents the kind of hashed value.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, IntoPrimitive)]
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
    /// The value is an `Input` (hidden type).
    Input,
    /// The value is an `Output` (hidden type).
    Output,
    /// The value is an `EnumVariant`.
    EnumVariant,
}

impl Hashable for ValueKind {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(&[(*self).into()]);
    }
}

/// Represents the kind of a content digest header.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, IntoPrimitive)]
#[repr(u8)]
enum ContentDigestKind {
    /// The content digest is from a hash algorithm.
    Hash,
    /// The content digest is from an ETag header.
    ETag,
}

impl Hashable for ContentDigestKind {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(&[(*self).into()]);
    }
}

/// Represents the kind of a hashed evaluation path.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, IntoPrimitive)]
#[repr(u8)]
enum PathKind {
    /// The path is to a local file.
    Local,
    /// The path is a URL to a remote file.
    Remote,
}

impl Hashable for PathKind {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(&[(*self).into()]);
    }
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

impl Hashable for &[u8] {
    fn hash(&self, hasher: &mut Hasher) {
        hasher.update(&(self.len() as u32).to_le_bytes());
        hasher.update(self);
    }
}

impl Hashable for &str {
    fn hash(&self, hasher: &mut Hasher) {
        self.as_bytes().hash(hasher);
    }
}

impl Hashable for String {
    fn hash(&self, hasher: &mut Hasher) {
        self.as_str().hash(hasher);
    }
}

impl Hashable for Path {
    fn hash(&self, hasher: &mut Hasher) {
        self.to_string_lossy().as_bytes().hash(hasher);
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
                ContentDigestKind::Hash.hash(hasher);
                algorithm.hash(hasher);
                digest.as_slice().hash(hasher);
            }
            cloud_copy::ContentDigest::ETag(etag) => {
                ContentDigestKind::ETag.hash(hasher);
                etag.hash(hasher);
            }
        }
    }
}

impl Hashable for Digest {
    fn hash(&self, hasher: &mut Hasher) {
        match self {
            Digest::File(digest) => {
                ContentKind::File.hash(hasher);
                digest.as_bytes().as_slice().hash(hasher);
            }
            Digest::Directory(digest) => {
                ContentKind::Directory.hash(hasher);
                digest.as_bytes().as_slice().hash(hasher);
            }
        }
    }
}

impl Hashable for EvaluationPath {
    fn hash(&self, hasher: &mut Hasher) {
        if let Some(path) = self.as_local() {
            PathKind::Local.hash(hasher);
            path.hash(hasher);
            return;
        }

        if let Some(url) = self.as_remote() {
            PathKind::Remote.hash(hasher);
            url.hash(hasher);
            return;
        }

        unreachable!("evaluation path should be either local or remote");
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
                ValueKind::None.hash(hasher);
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
            | Self::Call(_)
            | Self::TypeNameRef(_) => unreachable!("value cannot be hashed"),
        }
    }
}

impl Hashable for PrimitiveValue {
    fn hash(&self, hasher: &mut Hasher) {
        match self {
            Self::Boolean(v) => {
                ValueKind::Boolean.hash(hasher);
                hasher.update(&[if *v { 1u8 } else { 0u8 }]);
            }
            Self::Integer(v) => {
                ValueKind::Integer.hash(hasher);
                hasher.update(&v.to_le_bytes());
            }
            Self::Float(v) => {
                ValueKind::Float.hash(hasher);
                hasher.update(&v.to_le_bytes());
            }
            Self::String(v) => {
                ValueKind::String.hash(hasher);
                v.as_str().hash(hasher);
            }
            Self::File(v) => {
                ValueKind::File.hash(hasher);
                v.as_str().hash(hasher);
            }
            Self::Directory(v) => {
                ValueKind::Directory.hash(hasher);
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
            Self::EnumVariant(v) => v.hash(hasher),
        }
    }
}

impl Hashable for Pair {
    fn hash(&self, hasher: &mut Hasher) {
        ValueKind::Pair.hash(hasher);
        self.left().hash(hasher);
        self.right().hash(hasher);
    }
}

impl Hashable for Array {
    fn hash(&self, hasher: &mut Hasher) {
        ValueKind::Array.hash(hasher);
        hash_sequence(hasher, self.as_slice().iter());
    }
}

impl Hashable for Map {
    fn hash(&self, hasher: &mut Hasher) {
        ValueKind::Map.hash(hasher);
        hash_sequence(hasher, self.iter());
    }
}

impl Hashable for Object {
    fn hash(&self, hasher: &mut Hasher) {
        ValueKind::Object.hash(hasher);
        hash_sequence(hasher, self.iter());
    }
}

impl Hashable for Struct {
    fn hash(&self, hasher: &mut Hasher) {
        ValueKind::Struct.hash(hasher);
        hash_sequence(hasher, self.iter());
    }
}

impl Hashable for EnumVariant {
    fn hash(&self, hasher: &mut Hasher) {
        ValueKind::EnumVariant.hash(hasher);
        self.name().hash(hasher);
        self.value().hash(hasher);
    }
}

impl Hashable for HintsValue {
    fn hash(&self, hasher: &mut Hasher) {
        ValueKind::Hints.hash(hasher);
        hash_sequence(hasher, self.as_object().iter());
    }
}

impl Hashable for InputValue {
    fn hash(&self, hasher: &mut Hasher) {
        ValueKind::Input.hash(hasher);
        hash_sequence(hasher, self.as_object().iter());
    }
}

impl Hashable for OutputValue {
    fn hash(&self, hasher: &mut Hasher) {
        ValueKind::Output.hash(hasher);
        hash_sequence(hasher, self.as_object().iter());
    }
}

#[cfg(test)]
mod test {
    use blake3::Hash;
    use cloud_copy::ContentDigest;
    use indexmap::IndexMap;
    use pretty_assertions::assert_eq;
    use wdl_analysis::types::ArrayType;
    use wdl_analysis::types::MapType;
    use wdl_analysis::types::PairType;
    use wdl_analysis::types::PrimitiveType;
    use wdl_analysis::types::StructType;

    use super::*;

    #[test]
    fn hash_empty_sequence() {
        let mut hasher = Hasher::new();
        super::hash_sequence(&mut hasher, ([] as [&str; 0]).iter());
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&0u32.to_le_bytes()); // Count of elements
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_sequence() {
        let mut hasher = Hasher::new();
        super::hash_sequence(&mut hasher, ["foo", "bar", "baz"].iter());
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&3u32.to_le_bytes()); // Count of elements
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("foo".as_bytes()); // Literal string
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("bar".as_bytes()); // Literal string
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("baz".as_bytes()); // Literal string
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_bytes() {
        let mut hasher = Hasher::new();
        [0u8, 1, 2].as_slice().hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&3u32.to_le_bytes()); // Slice length
        hasher.update(&[0, 1, 2]); // Literal bytes
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_str() {
        let mut hasher = Hasher::new();
        "foo".hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("foo".as_bytes()); // Literal string
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_string() {
        let mut hasher = Hasher::new();
        "foo".to_string().hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("foo".as_bytes()); // Literal string
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_path() {
        let mut hasher = Hasher::new();
        Path::new("foo/bar").hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&7u32.to_le_bytes()); // String length
        hasher.update("foo/bar".as_bytes()); // Literal string
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_path_buf() {
        let mut hasher = Hasher::new();
        Path::new("foo/bar").to_path_buf().hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&7u32.to_le_bytes()); // String length
        hasher.update("foo/bar".as_bytes()); // Literal string
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_url() {
        let mut hasher = Hasher::new();
        "https://example.com/foo/bar"
            .parse::<Url>()
            .unwrap()
            .hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&27u32.to_le_bytes()); // String length
        hasher.update("https://example.com/foo/bar".as_bytes()); // Literal string
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_content_digest() {
        // ContentDigest::Hash variant
        let mut hasher = Hasher::new();
        ContentDigest::Hash {
            algorithm: "algo".to_string(),
            digest: [1, 2, 3].into(),
        }
        .hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[0]); // Hash tag
        hasher.update(&4u32.to_le_bytes()); // String length
        hasher.update("algo".as_bytes()); // Literal string
        hasher.update(&3u32.to_le_bytes()); // Slice length
        hasher.update(&[1, 2, 3]); // Literal bytes
        assert_eq!(hash, hasher.finalize());

        // ContentDigest::ETag variant
        let mut hasher = Hasher::new();
        ContentDigest::ETag("foo".to_string()).hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[1]); // ETag tag
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("foo".as_bytes()); // Literal string
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_digest() {
        // Digest::File variant
        let mut hasher = Hasher::new();
        // Blake3 hash of "hello world!"
        let expected =
            Hash::from_hex("3aa61c409fd7717c9d9c639202af2fae470c0ef669be7ba2caea5779cb534e9d")
                .unwrap();
        Digest::File(expected).hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[0]); // File tag
        hasher.update(&32u32.to_le_bytes()); // Slice length
        hasher.update(expected.as_bytes()); // Literal bytes
        assert_eq!(hash, hasher.finalize());

        // Digest::Directory variant
        let mut hasher = Hasher::new();
        Digest::Directory(expected).hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[1]); // Directory tag
        hasher.update(&32u32.to_le_bytes()); // Slice length
        hasher.update(expected.as_bytes()); // Literal bytes
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_evaluation_path() {
        // EvaluationPath::Local variant
        let mut hasher = Hasher::new();
        EvaluationPath::from_local_path("foo/bar".into()).hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[0]); // Local tag
        hasher.update(&7u32.to_le_bytes()); // String length
        hasher.update("foo/bar".as_bytes()); // Literal string
        assert_eq!(hash, hasher.finalize());

        // EvaluationPath::Remote variant
        let mut hasher = Hasher::new();
        EvaluationPath::try_from("https://example.com/foo".parse::<Url>().unwrap())
            .unwrap()
            .hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[1]); // Remote tag
        hasher.update(&23u32.to_le_bytes()); // String length
        hasher.update("https://example.com/foo".as_bytes()); // Literal string
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_optional_primitive() {
        // None variant
        let mut hasher = Hasher::new();
        None::<PrimitiveValue>.hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[0]); // WDL `None` tag
        assert_eq!(hash, hasher.finalize());

        // Some variant
        let mut hasher = Hasher::new();
        Some::<PrimitiveValue>(1234.into()).hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[2]); // WDL `Int` tag
        hasher.update(&1234i64.to_le_bytes()); // Literal integer
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_tuple() {
        let mut hasher = Hasher::new();
        ("foo", "bar").hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("foo".as_bytes()); // Literal string
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("bar".as_bytes()); // Literal string
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_wld_none() {
        let mut hasher = Hasher::new();
        Value::new_none(Type::None).hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[0]); // WDL `None` tag
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_wdl_boolean() {
        // A `false` WDL value
        let mut hasher = Hasher::new();
        PrimitiveValue::from(false).hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[1]); // WDL `Boolean` tag
        hasher.update(&[0]); // Literal false
        assert_eq!(hash, hasher.finalize());

        let mut hasher = Hasher::new();
        Value::from(false).hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[1]); // WDL `Boolean` tag
        hasher.update(&[0]); // Literal false
        assert_eq!(hash, hasher.finalize());

        // A `true` WDL value
        let mut hasher = Hasher::new();
        PrimitiveValue::from(true).hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[1]); // WDL `Boolean` tag
        hasher.update(&[1]); // Literal true
        assert_eq!(hash, hasher.finalize());

        let mut hasher = Hasher::new();
        Value::from(true).hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[1]); // WDL `Boolean` tag
        hasher.update(&[1]); // Literal true
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_wdl_integer() {
        let mut hasher = Hasher::new();
        PrimitiveValue::from(4321).hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[2]); // WDL `Int` tag
        hasher.update(&4321i64.to_le_bytes()); // Literal integer
        assert_eq!(hash, hasher.finalize());

        let mut hasher = Hasher::new();
        Value::from(4321).hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[2]); // WDL `Int` tag
        hasher.update(&4321i64.to_le_bytes()); // Literal integer
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_wdl_float() {
        let mut hasher = Hasher::new();
        PrimitiveValue::from(1.234).hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[3]); // WDL `Float` tag
        hasher.update(&1.234f64.to_le_bytes()); // Literal float
        assert_eq!(hash, hasher.finalize());

        let mut hasher = Hasher::new();
        Value::from(1.234).hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[3]); // WDL `Float` tag
        hasher.update(&1.234f64.to_le_bytes()); // Literal float
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_wdl_string() {
        let mut hasher = Hasher::new();
        PrimitiveValue::new_string("foo").hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[4]); // WDL `String` tag
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("foo".as_bytes()); // Literal string
        assert_eq!(hash, hasher.finalize());

        let mut hasher = Hasher::new();
        Value::from(PrimitiveValue::new_string("foo")).hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[4]); // WDL `String` tag
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("foo".as_bytes()); // Literal string
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_wdl_file() {
        let mut hasher = Hasher::new();
        PrimitiveValue::new_file("foo").hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[5]); // WDL `File` tag
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("foo".as_bytes()); // Literal string
        assert_eq!(hash, hasher.finalize());

        let mut hasher = Hasher::new();
        Value::from(PrimitiveValue::new_file("foo")).hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[5]); // WDL `File` tag
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("foo".as_bytes()); // Literal string
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_wdl_directory() {
        let mut hasher = Hasher::new();
        PrimitiveValue::new_directory("foo").hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[6]); // WDL `Directory` tag
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("foo".as_bytes()); // Literal string
        assert_eq!(hash, hasher.finalize());

        let mut hasher = Hasher::new();
        Value::from(PrimitiveValue::new_directory("foo")).hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[6]); // WDL `Directory` tag
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("foo".as_bytes()); // Literal string
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_wdl_pair() {
        let mut hasher = Hasher::new();
        Pair::new(
            PairType::new(PrimitiveType::String, PrimitiveType::Boolean),
            PrimitiveValue::new_string("foo"),
            false,
        )
        .unwrap()
        .hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[7]); // WDL `Pair` tag
        hasher.update(&[4]); // WDL `String` tag
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("foo".as_bytes()); // Literal string
        hasher.update(&[1]); // WDL `Boolean` tag
        hasher.update(&[0]); // Literal false
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_wdl_array() {
        let mut hasher = Hasher::new();
        Array::new(
            ArrayType::new(PrimitiveType::String),
            [
                PrimitiveValue::new_string("foo"),
                PrimitiveValue::new_string("bar"),
                PrimitiveValue::new_string("baz"),
            ],
        )
        .unwrap()
        .hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[8]); // WDL `Array` tag
        hasher.update(&3u32.to_le_bytes()); // Element count
        hasher.update(&[4]); // WDL `String` tag
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("foo".as_bytes()); // Literal string
        hasher.update(&[4]); // WDL `String` tag
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("bar".as_bytes()); // Literal string
        hasher.update(&[4]); // WDL `String` tag
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("baz".as_bytes()); // Literal string
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_wdl_map() {
        let mut hasher = Hasher::new();
        Map::new(
            MapType::new(PrimitiveType::Integer, PrimitiveType::Boolean),
            [(1, true), (2, false), (3, true)],
        )
        .unwrap()
        .hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[9]); // WDL `Map` tag
        hasher.update(&3u32.to_le_bytes()); // Element count
        hasher.update(&[2]); // WDL `Int` tag
        hasher.update(&1i64.to_le_bytes()); // Literal integer
        hasher.update(&[1]); // WDL `Boolean` tag
        hasher.update(&[1]); // Literal true
        hasher.update(&[2]); // WDL `Int` tag
        hasher.update(&2i64.to_le_bytes()); // Literal integer
        hasher.update(&[1]); // WDL `Boolean` tag
        hasher.update(&[0]); // Literal false
        hasher.update(&[2]); // WDL `Int` tag
        hasher.update(&3i64.to_le_bytes()); // Literal integer
        hasher.update(&[1]); // WDL `Boolean` tag
        hasher.update(&[1]); // Literal true
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_wdl_object() {
        let mut hasher = Hasher::new();
        Object::new(IndexMap::from_iter([
            ("foo".to_string(), 1234.into()),
            ("bar".to_string(), Value::new_none(Type::None)),
            ("baz".to_string(), false.into()),
        ]))
        .hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[10]); // WDL `Object` tag
        hasher.update(&3u32.to_le_bytes()); // Element count
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("foo".as_bytes()); // Literal string
        hasher.update(&[2]); // WDL `Int` tag
        hasher.update(&1234i64.to_le_bytes()); // String length
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("bar".as_bytes()); // Literal string
        hasher.update(&[0]); // WDL `None` tag
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("baz".as_bytes()); // Literal string
        hasher.update(&[1]); // WDL `Boolean` tag
        hasher.update(&[0]); // Literal false
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_wdl_struct() {
        let mut hasher = Hasher::new();
        Struct::new(
            StructType::new(
                "Foo",
                [
                    ("foo", PrimitiveType::Boolean),
                    ("bar", PrimitiveType::String),
                    ("baz", PrimitiveType::Float),
                ],
            ),
            [
                ("foo", Value::from(true)),
                ("bar", PrimitiveValue::new_string("foo").into()),
                ("baz", Value::from(1234.56)),
            ],
        )
        .unwrap()
        .hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[11]); // WDL `Struct` tag
        hasher.update(&3u32.to_le_bytes()); // Field count
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("foo".as_bytes()); // Literal string
        hasher.update(&[1]); // WDL `Boolean` tag
        hasher.update(&[1]); // Literal boolean
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("bar".as_bytes()); // Literal string
        hasher.update(&[4]); // WDL `String` tag
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("foo".as_bytes()); // Literal string
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("baz".as_bytes()); // Literal string
        hasher.update(&[3]); // WDL `Float` tag
        hasher.update(&1234.56f64.to_le_bytes()); // Literal float
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_wdl_hints() {
        let mut hasher = Hasher::new();
        HintsValue::from(Object::new(IndexMap::from_iter([
            ("foo".to_string(), 1234.into()),
            ("bar".to_string(), Value::new_none(Type::None)),
            ("baz".to_string(), false.into()),
        ])))
        .hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[12]); // WDL `Hints` tag
        hasher.update(&3u32.to_le_bytes()); // Element count
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("foo".as_bytes()); // Literal string
        hasher.update(&[2]); // WDL `Int` tag
        hasher.update(&1234i64.to_le_bytes()); // String length
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("bar".as_bytes()); // Literal string
        hasher.update(&[0]); // WDL `None` tag
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("baz".as_bytes()); // Literal string
        hasher.update(&[1]); // WDL `Boolean` tag
        hasher.update(&[0]); // Literal false
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_wdl_input() {
        let mut hasher = Hasher::new();
        InputValue::from(Object::new(IndexMap::from_iter([
            ("foo".to_string(), 1234.into()),
            ("bar".to_string(), Value::new_none(Type::None)),
            ("baz".to_string(), false.into()),
        ])))
        .hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[13]); // WDL `Input` tag
        hasher.update(&3u32.to_le_bytes()); // Element count
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("foo".as_bytes()); // Literal string
        hasher.update(&[2]); // WDL `Int` tag
        hasher.update(&1234i64.to_le_bytes()); // String length
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("bar".as_bytes()); // Literal string
        hasher.update(&[0]); // WDL `None` tag
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("baz".as_bytes()); // Literal string
        hasher.update(&[1]); // WDL `Boolean` tag
        hasher.update(&[0]); // Literal false
        assert_eq!(hash, hasher.finalize());
    }

    #[test]
    fn hash_wdl_output() {
        let mut hasher = Hasher::new();
        OutputValue::from(Object::new(IndexMap::from_iter([
            ("foo".to_string(), 1234.into()),
            ("bar".to_string(), Value::new_none(Type::None)),
            ("baz".to_string(), false.into()),
        ])))
        .hash(&mut hasher);
        let hash = hasher.finalize();

        let mut hasher = Hasher::new();
        hasher.update(&[14]); // WDL `Output` tag
        hasher.update(&3u32.to_le_bytes()); // Element count
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("foo".as_bytes()); // Literal string
        hasher.update(&[2]); // WDL `Int` tag
        hasher.update(&1234i64.to_le_bytes()); // String length
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("bar".as_bytes()); // Literal string
        hasher.update(&[0]); // WDL `None` tag
        hasher.update(&3u32.to_le_bytes()); // String length
        hasher.update("baz".as_bytes()); // Literal string
        hasher.update(&[1]); // WDL `Boolean` tag
        hasher.update(&[0]); // Literal false
        assert_eq!(hash, hasher.finalize());
    }
}

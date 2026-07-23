//! Generic YAML parsing utilities for Sprocket test definitions.

use std::cmp::Ordering;
use std::hash::Hash;
use std::hash::Hasher;
use std::marker::PhantomData;

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Deserializer;
use serde_json::Number;
use serde_json::Value;

/// A macro to generate a struct with named YAML fields and a catch-all
/// `unknown_fields` field.
///
/// Each field in the struct must be wrapped in an `Option<SpannedField>`,
/// allowing the struct to retain its corresponding field key and value. The
/// same applies to the `unknown_fields`, which stores any unmapped key-value
/// pairs.
macro_rules! spanned_fields {
    (
        $(#[$meta:meta])*
        $vis:vis struct $struct_name:ident {
            $(
                $field_vis:vis $field_name:ident : $field_ty:ty,
            )* $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis struct $struct_name {
            $(
                $field_vis $field_name : $field_ty,
            )*
            $vis unknown_fields: std::collections::BTreeMap<Spanned<String>, Value>,
        }

        impl<'de> serde::de::Deserialize<'de> for $struct_name {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
            where
                D: serde::de::Deserializer<'de>,
            {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = $struct_name;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        formatter.write_str(concat!("a mapping of ", stringify!($struct_name), " fields"))
                    }

                    fn visit_map<V>(self, mut map: V) -> Result<$struct_name, V::Error>
                    where
                        V: serde::de::MapAccess<'de>,
                    {
                        $(
                            // Initialize all defined fields to None
                            let mut $field_name = None;
                        )*
                        let mut unknown_fields = BTreeMap::new();

                        while let Some(key) = map.next_key::<Spanned<String>>()? {
                            match key.0.value.as_str() {
                                $(
                                    stringify!($field_name) => {
                                        $field_name = Some(SpannedField {
                                            key,
                                            value: map.next_value()?,
                                        });
                                    }
                                )*
                                _ => {
                                    unknown_fields.insert(key, map.next_value()?);
                                }
                            }
                        }

                        Ok($struct_name {
                            $(
                                $field_name,
                            )*
                            unknown_fields,
                        })
                    }
                }

                deserializer.deserialize_map(Visitor)
            }
        }
    };
}

pub(crate) use spanned_fields;

/// A YAML value that retains a span to its field key.
#[derive(Debug, Clone, JsonSchema)]
#[schemars(inline, with = "T")]
pub struct SpannedField<T> {
    /// The name of the field.
    pub key: Spanned<String>,
    /// The value of the field.
    pub value: T,
}

/// Wrapper around [`serde_saphyr::Spanned`] that provides extra trait
/// implementations.
#[derive(Debug, Clone, JsonSchema)]
#[repr(transparent)]
#[schemars(with = "T")]
pub struct Spanned<T>(pub serde_saphyr::Spanned<T>);

impl<T> PartialEq for Spanned<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.0.value == other.0.value
    }
}

impl<T> Eq for Spanned<T> where T: PartialEq {}

impl<T> PartialOrd for Spanned<T>
where
    T: PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.0.value.partial_cmp(&other.0.value)
    }
}

impl<T> Ord for Spanned<T>
where
    T: Ord,
{
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.value.cmp(&other.0.value)
    }
}

impl<T> Hash for Spanned<T>
where
    T: Hash,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.value.hash(state);
    }
}

impl<'de, T> Deserialize<'de> for Spanned<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> anyhow::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        serde_saphyr::Spanned::<T>::deserialize(deserializer).map(Spanned)
    }
}

/// A value that *might* be a YAML mapping.
#[derive(Debug)]
pub(crate) enum MaybeMap<V> {
    /// The value is a mapping.
    Map(IndexMap<Spanned<String>, Spanned<V>>),
    /// The value is something else.
    Other(Value),
}

// Unfortunately, these manual `Deserialize` impls are needed (as opposed to a
// `#[serde(untagged)]` impl). See <https://docs.rs/serde-saphyr/latest/serde_saphyr/struct.Spanned.html#limitation-with-serdeflatten-serdeuntagged-and-serdetag-->
impl<'de, V> Deserialize<'de> for MaybeMap<V>
where
    V: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MaybeMapVisitor<V>(PhantomData<V>);

        impl<'de, V> serde::de::Visitor<'de> for MaybeMapVisitor<V>
        where
            V: Deserialize<'de>,
        {
            type Value = MaybeMap<V>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a YAML mapping")
            }

            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(MaybeMap::Other(Value::Bool(v)))
            }

            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(MaybeMap::Other(Value::Number(v.into())))
            }

            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(MaybeMap::Other(Value::Number(v.into())))
            }

            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                let Some(v) = Number::from_f64(v) else {
                    return Err(E::custom("invalid floating point number"));
                };

                Ok(MaybeMap::Other(Value::Number(v)))
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(MaybeMap::Other(Value::String(v.to_owned())))
            }

            fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let v = Value::deserialize(serde::de::value::SeqAccessDeserializer::new(seq))?;
                Ok(MaybeMap::Other(v))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut index_map = match map.size_hint() {
                    Some(size) => IndexMap::with_capacity(size),
                    None => IndexMap::new(),
                };

                while let Some((key, value)) = map.next_entry::<Spanned<String>, Spanned<V>>()? {
                    index_map.insert(key, value);
                }

                Ok(MaybeMap::Map(index_map))
            }
        }

        deserializer.deserialize_any(MaybeMapVisitor(PhantomData))
    }
}

/// A value that *might* be a YAML sequence.
#[derive(Debug)]
pub(crate) enum MaybeSequence<T> {
    /// The value is a sequence.
    Seq(Vec<Spanned<T>>),
    /// The value is something else.
    Other(Value),
}

impl<'de, T> Deserialize<'de> for MaybeSequence<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MaybeSeqVisitor<T>(PhantomData<T>);

        impl<'de, T> serde::de::Visitor<'de> for MaybeSeqVisitor<T>
        where
            T: Deserialize<'de>,
        {
            type Value = MaybeSequence<T>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a YAML sequence")
            }

            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(MaybeSequence::Other(Value::Bool(v)))
            }

            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(MaybeSequence::Other(Value::Number(v.into())))
            }

            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(MaybeSequence::Other(Value::Number(v.into())))
            }

            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                let Some(v) = Number::from_f64(v) else {
                    return Err(E::custom("invalid floating point number"));
                };

                Ok(MaybeSequence::Other(Value::Number(v)))
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(MaybeSequence::Other(Value::String(v.to_owned())))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut vec = Vec::new();
                while let Some(elem) = seq.next_element::<Spanned<T>>()? {
                    vec.push(elem);
                }
                Ok(MaybeSequence::Seq(vec))
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let v = Value::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                Ok(MaybeSequence::Other(v))
            }
        }

        deserializer.deserialize_any(MaybeSeqVisitor(PhantomData))
    }
}

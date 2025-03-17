//! URIs for the `container` item within the `runtime` and `requirements`
//! blocks.

use std::ops::Deref;
use std::str::FromStr;

use wdl_grammar::SyntaxNode;

use crate::AstNode;
use crate::AstToken;
use crate::TreeNode;
use crate::v1::LiteralString;

/// The value of the key that signifies _any_ POSIX-compliant operating
/// environment may be used.
pub const ANY_CONTAINER_VALUE: &str = "*";

/// The default protocol for a container URI.
const DEFAULT_PROTOCOL: &str = "docker";

/// The separator for the protocol section within a container URI.
const PROTOCOL_SEPARATOR: &str = "://";

/// The separator within the location that splits the image identifier from the
/// tag.
const TAG_SEPARATOR: &str = ":";

/// The token that specifies whether an image points to an immutable sha256 tag.
const SHA256_TOKEN: &str = "@sha256:";

/// An error related to a [`Uri`].
#[derive(Debug)]
pub enum Error {
    /// An empty tag was encountered.
    EmptyTag,

    /// Attempted to create a [`Uri`] from an interpolated, literal string.
    Interpolated(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::EmptyTag => write!(f, "tag of a container URI cannot be empty"),
            Error::Interpolated(s) => write!(
                f,
                "cannot create a uri from an interpolated string literal: {s}",
            ),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// The protocol portion of the container URI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Protocol(String);

impl std::ops::Deref for Protocol {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Default for Protocol {
    fn default() -> Self {
        Self(String::from(DEFAULT_PROTOCOL))
    }
}

/// The location portion of the container URI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Location {
    /// The textual offset of the location within the parent [`Uri`].
    offset_within_parent: usize,

    /// The entire location.
    value: String,

    /// The offset at which the image portion of the location ends within the
    /// value.
    image_end: usize,

    /// The offset at which the tag portion of the location starts within the
    /// value.
    tag_start: Option<usize>,

    /// Whether or not the location is immutable.
    immutable: bool,
}

impl Location {
    /// Attempts to create a new [`Location`].
    ///
    /// # Errors
    ///
    /// * If the tag is empty, an [`Error::EmptyTag`] will be returned.
    pub fn try_new(value: String, offset_within_parent: usize) -> Result<Self> {
        let immutable = value.contains(SHA256_TOKEN);

        let tag_start = value
            .find(TAG_SEPARATOR)
            .map(|offset| offset + TAG_SEPARATOR.len())
            .map(|offset| {
                if value[offset..].is_empty() {
                    Err(Error::EmptyTag)
                } else {
                    Ok(offset)
                }
            })
            .transpose()?;

        let image_end = if let Some(sha_offset) = value.find(SHA256_TOKEN) {
            sha_offset
        } else if let Some(tag_start) = tag_start {
            tag_start - TAG_SEPARATOR.len()
        } else {
            value.len()
        };

        Ok(Self {
            offset_within_parent,
            value,
            image_end,
            tag_start,
            immutable,
        })
    }

    /// Gets the textual offset of the location within the parent [`Uri`].
    pub fn offset_within_parent(&self) -> usize {
        self.offset_within_parent
    }

    /// Gets the image portion of the location.
    pub fn image(&self) -> &str {
        &self.value[..self.image_end]
    }

    /// Gets the tag portion of the location (if it exists).
    pub fn tag(&self) -> Option<&str> {
        if let Some(offset) = self.tag_start {
            Some(&self.value[offset..])
        } else {
            None
        }
    }

    /// Gets whether the location is immutable.
    pub fn immutable(&self) -> bool {
        self.immutable
    }
}

impl std::ops::Deref for Location {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

/// An individual URI entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    /// The protocol.
    protocol: Option<Protocol>,

    /// The location.
    location: Location,
}

impl Entry {
    /// Gets a reference to the protocol.
    pub fn protocol(&self) -> Option<&Protocol> {
        self.protocol.as_ref()
    }

    /// Gets a reference to the location.
    pub fn location(&self) -> &Location {
        &self.location
    }

    /// Gets the image name.
    pub fn image(&self) -> &str {
        self.location.image()
    }

    /// Gets the tag (if it exists).
    pub fn tag(&self) -> Option<&str> {
        self.location.tag()
    }

    /// Gets whether the [`Entry`] is immutable.
    pub fn immutable(&self) -> bool {
        self.location.immutable()
    }
}

/// A kind of container URI as defined by the WDL specification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Kind {
    /// Any POSIX-compliant operating environment the executor wishes.
    Any,

    /// A container URI entry.
    Entry(Entry),
}

impl Kind {
    /// Returns whether this kind is a [`Kind::Any`].
    pub fn is_any(&self) -> bool {
        matches!(self, Kind::Any)
    }

    /// Returns whether this kind is a [`Kind::Entry`].
    pub fn is_entry(&self) -> bool {
        matches!(self, Kind::Entry(_))
    }

    /// Attempts to return a reference to the inner [`Entry`].
    ///
    /// - If the value is an [`Kind::Entry`], a reference to the inner [`Entry`]
    ///   is returned.
    /// - Else, [`None`] is returned.
    pub fn as_entry(&self) -> Option<&Entry> {
        match self {
            Kind::Entry(entry) => Some(entry),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`Entry`].
    ///
    /// - If the value is a [`Kind::Entry`], the inner [`Entry`] is returned.
    /// - Else, [`None`] is returned.
    pub fn into_entry(self) -> Option<Entry> {
        match self {
            Kind::Entry(entry) => Some(entry),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`Entry`].
    ///
    /// # Panics
    ///
    /// Panics if the kind is not a [`Kind::Entry`].
    pub fn unwrap_entry(self) -> Entry {
        self.into_entry().expect("uri kind is not an entry")
    }
}

impl FromStr for Kind {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        if text == ANY_CONTAINER_VALUE {
            return Ok(Kind::Any);
        }

        let (protocol, location_offset, location) = match text.find(PROTOCOL_SEPARATOR) {
            Some(offset) => {
                let location_offset = offset + PROTOCOL_SEPARATOR.len();
                (
                    Some(&text[..offset]),
                    location_offset,
                    &text[location_offset..],
                )
            }
            None => (None, 0, text),
        };

        let protocol = protocol.map(|s| Protocol(String::from(s)));
        let location = Location::try_new(String::from(location), location_offset)?;

        Ok(Kind::Entry(Entry { protocol, location }))
    }
}

/// A container URI as defined by the WDL specification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Uri<N: TreeNode = SyntaxNode> {
    /// The kind of the container URI.
    kind: Kind,

    /// The literal string backing this container URI.
    literal_string: LiteralString<N>,
}

impl<N: TreeNode> Uri<N> {
    /// Gets the kind of the [`Uri`].
    pub fn kind(&self) -> &Kind {
        &self.kind
    }

    /// Consumes `self` and returns the kind of the [`Uri`].
    pub fn into_kind(self) -> Kind {
        self.kind
    }

    /// Gets the backing literal string of the [`Uri`].
    pub fn literal_string(&self) -> &LiteralString<N> {
        &self.literal_string
    }

    /// Consumes `self` and returns the literal string backing the [`Uri`].
    pub fn into_literal_string(self) -> LiteralString<N> {
        self.literal_string
    }

    /// Consumes `self` and returns the parts of the [`Uri`].
    pub fn into_parts(self) -> (Kind, LiteralString<N>) {
        (self.kind, self.literal_string)
    }
}

impl<N: TreeNode> Deref for Uri<N> {
    type Target = Kind;

    fn deref(&self) -> &Self::Target {
        &self.kind
    }
}

impl<N: TreeNode> TryFrom<LiteralString<N>> for Uri<N> {
    type Error = Error;

    fn try_from(literal_string: LiteralString<N>) -> Result<Self> {
        let kind = literal_string
            .text()
            .ok_or_else(|| Error::Interpolated(literal_string.inner().text().to_string()))?
            .text()
            .parse::<Kind>()?;

        Ok(Uri {
            kind,
            literal_string,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_uri_kind() {
        let kind = "*".parse::<Kind>().expect("kind to parse");
        assert!(kind.is_any());
    }

    #[test]
    fn standard_uri_kind() {
        let entry = "ubuntu:latest"
            .parse::<Kind>()
            .expect("kind to parse")
            .unwrap_entry();

        assert!(entry.protocol().is_none());
        assert_eq!(entry.location().as_str(), "ubuntu:latest");
        assert_eq!(entry.location().image(), "ubuntu");
        assert_eq!(entry.location().tag().unwrap(), "latest");
        assert!(!entry.location().immutable());
    }

    #[test]
    fn standard_uri_kind_with_protocol() {
        let entry = "docker://ubuntu:latest"
            .parse::<Kind>()
            .expect("uri to parse")
            .unwrap_entry();

        assert_eq!(entry.protocol().unwrap().as_str(), "docker");
        assert_eq!(entry.location().as_str(), "ubuntu:latest");
        assert_eq!(entry.location().image(), "ubuntu");
        assert_eq!(entry.location().tag().unwrap(), "latest");
        assert!(!entry.location().immutable());
    }

    #[test]
    fn standard_uri_kind_with_protocol_and_immutable_tag() {
        let entry = "docker://ubuntu@sha256:abcd1234"
            .parse::<Kind>()
            .expect("uri to parse")
            .into_entry()
            .expect("uri to be an entry");
        assert_eq!(entry.protocol().unwrap().as_str(), "docker");
        assert_eq!(entry.location().as_str(), "ubuntu@sha256:abcd1234");
        assert_eq!(entry.location().image(), "ubuntu");
        assert_eq!(entry.location().tag().unwrap(), "abcd1234");
        assert!(entry.location().immutable());
    }

    #[test]
    fn standard_uri_kind_with_protocol_without_tag() {
        let entry = "docker://ubuntu"
            .parse::<Kind>()
            .expect("uri to parse")
            .unwrap_entry();

        assert_eq!(entry.protocol().unwrap().as_str(), "docker");
        assert_eq!(entry.location().as_str(), "ubuntu");
        assert_eq!(entry.location().image(), "ubuntu");
        assert!(entry.location().tag().is_none());
        assert!(!entry.location().immutable());
    }

    #[test]
    fn empty_tag() {
        let err = "docker://ubuntu:".parse::<Kind>().unwrap_err();
        assert!(matches!(err, Error::EmptyTag));

        let err = "ubuntu:".parse::<Kind>().unwrap_err();
        assert!(matches!(err, Error::EmptyTag));
    }
}

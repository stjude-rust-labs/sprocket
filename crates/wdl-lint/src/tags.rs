//! Definition of lint rule tags.

use strum::VariantArray;

/// A lint rule tag.
#[derive(VariantArray, Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum Tag {
    /// Rules associated with having a complete document.
    Completeness,

    /// Rules associated with the names of WDL elements.
    Naming,

    /// Rules associated with the whitespace in a document.
    Spacing,

    /// Rules associated with the style of a document.
    Style,

    /// Rules associated with the clarity of a document.
    Clarity,

    /// Rules associated with the portability of a document.
    Portability,

    /// Rules associated with the correctness of a document.
    Correctness,

    /// Rules associated with sorting of document elements.
    Sorting,

    /// Rules associated with the use of deprecated language constructs.
    Deprecated,

    /// Rules associeted with documentation.
    Documentation,

    /// Rules associated with compatibility with documentation rendering.
    DocRendering,
}

/// An error for when an unknown tag is encountered.
#[derive(Debug)]
pub struct UnknownTagError(String);

impl std::fmt::Display for UnknownTagError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown tag: {}", self.0)
    }
}

impl std::error::Error for UnknownTagError {}

impl std::str::FromStr for Tag {
    type Err = UnknownTagError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            s if s.eq_ignore_ascii_case("completeness") => Ok(Self::Completeness),
            s if s.eq_ignore_ascii_case("naming") => Ok(Self::Naming),
            s if s.eq_ignore_ascii_case("spacing") => Ok(Self::Spacing),
            s if s.eq_ignore_ascii_case("style") => Ok(Self::Style),
            s if s.eq_ignore_ascii_case("clarity") => Ok(Self::Clarity),
            s if s.eq_ignore_ascii_case("portability") => Ok(Self::Portability),
            s if s.eq_ignore_ascii_case("correctness") => Ok(Self::Correctness),
            s if s.eq_ignore_ascii_case("sorting") => Ok(Self::Sorting),
            s if s.eq_ignore_ascii_case("deprecated") => Ok(Self::Deprecated),
            s if s.eq_ignore_ascii_case("documentation") => Ok(Self::Documentation),
            s if s.eq_ignore_ascii_case("docrendering") => Ok(Self::DocRendering),
            _ => Err(UnknownTagError(s.to_string())),
        }
    }
}

impl std::fmt::Display for Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Completeness => write!(f, "Completeness"),
            Self::Naming => write!(f, "Naming"),
            Self::Spacing => write!(f, "Spacing"),
            Self::Style => write!(f, "Style"),
            Self::Clarity => write!(f, "Clarity"),
            Self::Portability => write!(f, "Portability"),
            Self::Correctness => write!(f, "Correctness"),
            Self::Sorting => write!(f, "Sorting"),
            Self::Deprecated => write!(f, "Deprecated"),
            Self::Documentation => write!(f, "Documentation"),
            Self::DocRendering => write!(f, "DocRendering"),
        }
    }
}

/// A set of lint tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TagSet(u32);

impl TagSet {
    /// Constructs a tag set from a slice of tags.
    pub const fn new(tags: &[Tag]) -> Self {
        if tags.is_empty() {
            return Self(0);
        }

        let mut bits = 0u32;
        let mut i = 0;
        while i < tags.len() {
            bits |= Self::mask(tags[i]);
            i += 1;
        }
        Self(bits)
    }

    /// Unions two tag sets together.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Intersects two tag sets.
    pub const fn intersect(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// Checks if the tag is contained in the set.
    pub const fn contains(&self, tag: Tag) -> bool {
        self.0 & Self::mask(tag) != 0
    }

    /// Gets the count of tags in the set.
    pub const fn count(&self) -> usize {
        self.0.count_ones() as usize
    }

    /// Masks the given tag to a `u32`.
    const fn mask(tag: Tag) -> u32 {
        1u32 << (tag as u8)
    }

    /// Iterates the tags in the set.
    pub fn iter(&self) -> impl Iterator<Item = Tag> + use<> {
        let mut bits = self.0;
        std::iter::from_fn(move || {
            if bits == 0 {
                return None;
            }

            let tag = unsafe {
                std::mem::transmute::<u8, Tag>(
                    u8::try_from(bits.trailing_zeros())
                        .expect("the maximum tag value should be less than 32"),
                )
            };

            bits ^= bits & bits.overflowing_neg().0;
            Some(tag)
        })
    }
}

/// Display for a tag set.
impl std::fmt::Display for TagSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut tags = self.iter().collect::<Vec<_>>();
        tags.sort();
        write!(f, "{tags:?}")
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn it_unions() {
        let a = TagSet::new(&[Tag::Clarity, Tag::Completeness]);
        assert_eq!(a.count(), 2);
        let b = TagSet::new(&[Tag::Clarity, Tag::Deprecated]);
        assert_eq!(b.count(), 2);

        let union = a.union(b);
        assert_eq!(
            union,
            TagSet::new(&[Tag::Clarity, Tag::Completeness, Tag::Deprecated])
        );
        assert_eq!(union.count(), 3);
    }

    #[test]
    fn it_intersects() {
        let a = TagSet::new(&[Tag::Clarity, Tag::Completeness]);
        assert_eq!(a.count(), 2);
        let b = TagSet::new(&[Tag::Clarity, Tag::Deprecated]);
        assert_eq!(b.count(), 2);

        let intersection = a.intersect(b);

        assert_eq!(intersection, TagSet::new(&[Tag::Clarity]));
        assert_eq!(intersection.count(), 1);
    }

    #[test]
    fn empty_slice_behaves() {
        let a = TagSet::new(&[]);
        assert_eq!(a.0, 0u32);

        let b = TagSet::new(&[]);
        assert_eq!(a, b);
        assert_eq!(a, b.intersect(a));
        assert_eq!(b, a.union(b));
        assert_eq!(a.count(), 0);
    }
}

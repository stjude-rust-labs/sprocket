//! Definition of lint rule tags.

use strum::Display;
use strum::EnumCount;
use strum::EnumString;
use strum::VariantArray;

/// A lint rule tag.
#[repr(u8)]
#[derive(
    Copy,
    Clone,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Display,
    EnumCount,
    EnumString,
    VariantArray,
)]
#[strum(ascii_case_insensitive, parse_err_ty = UnknownTagError, parse_err_fn = parse_tag_err)]
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

    /// Rules associated with documentation.
    Documentation,

    /// Rules associated with keeping WDL compatible with other Sprocket
    /// commands (e.g. `doc`).
    SprocketCompatibility,

    /// Rules associated with the performance of a document.
    Performance,

    /// Rules that may be overly strict or produce false positives.
    Pedantic,

    // NOTE: This **must** be the last variant. It gets special treatment in `TagSet`.
    /// Rules from all tags.
    All,
}

const _: () = {
    assert!(
        Tag::All as usize == Tag::COUNT - 1,
        "`Tag::All` must be the last variant"
    );

    assert!(Tag::COUNT < 32, "`Tag` has too many variants");
};

/// An error for when an unknown tag is encountered.
#[derive(Debug)]
pub struct UnknownTagError(String);

impl std::fmt::Display for UnknownTagError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown tag: {}", self.0)
    }
}

impl std::error::Error for UnknownTagError {}

/// Create an `UnknownTagError` for a tag.
fn parse_tag_err(tag: &str) -> UnknownTagError {
    UnknownTagError(tag.to_string())
}

/// A set of lint tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TagSet(u32);

impl TagSet {
    /// A tag set containing all tags.
    pub const ALL: Self = Self({
        let mut bits = 0;
        let mut i = 0;
        while i < Tag::All as u8 {
            bits |= 1 << i;
            i += 1;
        }
        bits
    });
    /// An empty tag set.
    pub const EMPTY: Self = Self(0);

    /// Constructs a tag set from a slice of tags.
    pub const fn new(tags: &[Tag]) -> Self {
        if tags.is_empty() {
            return Self(0);
        }

        let mut ret = Self::EMPTY;
        let mut i = 0;
        while i < tags.len() {
            let tag = tags[i];

            // `Tag::All` is just a marker, it shouldn't show up in the set.
            if tag as u8 == Tag::All as u8 {
                return Self::ALL;
            }

            ret.add(tag);
            i += 1;
        }
        ret
    }

    /// Add a tag to the set.
    pub const fn add(&mut self, tag: Tag) {
        self.0 |= Self::mask(tag);
    }

    /// Unions two tag sets together.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Intersects two tag sets.
    pub const fn intersect(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// Subtracts the `other` set from this set.
    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
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

impl std::fmt::Display for TagSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl Extend<Tag> for TagSet {
    fn extend<T: IntoIterator<Item = Tag>>(&mut self, iter: T) {
        for tag in iter {
            self.add(tag);
        }
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
    fn it_diffs() {
        let a = TagSet::new(&[Tag::Clarity, Tag::Completeness]);
        assert_eq!(a.count(), 2);

        let b = TagSet::new(&[Tag::Clarity, Tag::Deprecated]);
        assert_eq!(b.count(), 2);

        let diff = a.difference(b);

        assert_eq!(diff, TagSet::new(&[Tag::Completeness]));
        assert_eq!(diff.count(), 1);
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

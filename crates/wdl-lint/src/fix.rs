//! Module for applying fixes for diagnostics.

use std::ops::Range;

use ftree::FenwickTree;
use serde::Deserialize;

/// An insertion point.
#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsertionPoint {
    /// Insert immediately before a specified region.
    BeforeStart,
    /// Insert immediately after a specified region.
    AfterEnd,
}

/// A replacement to be applied to a String.
#[derive(Clone, Debug)]
pub struct Replacement {
    /// The start position of the replacement.
    start: usize,
    /// The end position of the replacement.
    end: usize,
    /// where to insert the replacement.
    insertion_point: InsertionPoint,
    /// Value to be inserted.
    value: String,
    /// Precedence for replacement. Higher precedences are applied first.
    precedence: usize,
}

impl Replacement {
    /// Create a new `Replacement`.
    pub fn new(
        start: usize,
        end: usize,
        insertion_point: InsertionPoint,
        value: String,
        precedence: usize,
    ) -> Self {
        Replacement {
            start,
            end,
            insertion_point,
            value,
            precedence,
        }
    }

    /// The start position of the replacement.
    pub fn start(&self) -> usize {
        self.start
    }

    /// The end position of the replacement.
    pub fn end(&self) -> usize {
        self.end
    }

    /// where to insert the replacement.
    pub fn insertion_point(&self) -> InsertionPoint {
        self.insertion_point
    }

    /// Value to be inserted.
    pub fn value(&self) -> &str {
        self.value.as_ref()
    }

    /// Precedence for replacement. Higher precedences are applied first.
    pub fn precedence(&self) -> usize {
        self.precedence
    }
}

// Adapted from ShellCheck's [Fixer](https://github.com/koalaman/shellcheck/blob/master/src/ShellCheck/Fixer.hs)
/// Apply a series of `Replacement`s to a String.
///
/// Internally uses a [Fenwick Tree](https://en.wikipedia.org/wiki/Fenwick_tree)
/// which is updated as replacements are applied. This allows multiple
/// replacements referencing only the original input. Although the canonical
/// Fenwick tree is 1-indexed this implementation is 0-indexed, so replacement
/// positions must be directly equivalent to string indices.
///
/// The length of the tree is initialized to be 1 longer
/// than the length of the initial value. This is because ftree provides
/// no API for accessing the value of the final position, and the prefix sum
/// only provides the cumulative sum < index. The extra index makes it possible
/// to calculate the sum of the entire tree, which is necessary to enable
/// slices of the new value beyond the original end position.
/// Attempting to apply a replacement at this position will panic.
#[derive(Clone, Debug)]
pub struct Fixer {
    /// The string to be modified.
    value: String,
    /// A Fenwick tree for tracking modifications.
    tree: FenwickTree<i32>,
}

#[allow(unused)]
impl Fixer {
    /// Create a new Fixer from a String.
    pub fn new(value: String) -> Self {
        Fixer {
            tree: FenwickTree::from_iter(vec![0; value.len() + 1]),
            value,
        }
    }

    /// Apply a [`Replacement`] to the value contained in the Fixer.
    ///
    /// Panics if the replacement is out-of-bounds.
    pub fn apply_replacement(&mut self, replacement: &Replacement) {
        let old_start = replacement.start;
        let old_end = replacement.end;
        let new_start = self.transform(old_start);
        let new_end = self.transform(old_end);

        let rep_len =
            i32::try_from(replacement.value().len()).expect("replacement length fits into i32");
        let range = i32::try_from(old_end - old_start).expect("range fits into i32");
        let shift = rep_len - range;
        let insert_at = match replacement.insertion_point() {
            InsertionPoint::BeforeStart => old_start,
            InsertionPoint::AfterEnd => old_end + 1,
        };
        // The final position in the tree is reserved
        // to work around the ftree API and is not
        // a valid insertion point.
        assert!(
            insert_at <= self.tree().len(),
            "attempt to insert out-of-bounds"
        );
        self.tree.add_at(insert_at, shift);
        self.value
            .replace_range(new_start..new_end, &replacement.value);
    }

    /// Apply multiple [`Replacement`]s in the correct order.
    ///
    /// Order is determined by the precedence field.
    /// Higher precedences are applied first.
    pub fn apply_replacements(&mut self, mut reps: Vec<Replacement>) {
        reps.sort_by_key(|r| r.precedence());
        reps.iter().rev().for_each(|r| self.apply_replacement(r));
    }

    /// Returns a reference to the value of the fixer with any applied
    /// replacements.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Given a `Range`, update the bounds to account for any applied
    /// replacements.
    ///
    /// Panics if the input does not fall within the bounds of the Fixer's
    /// value or if the adjusted index does not fit within usize.
    pub fn adjust_range(&self, range: Range<usize>) -> Range<usize> {
        self.transform(range.start)..self.transform(range.end)
    }

    /// Returns a reference to the internal Fenwick tree.
    pub fn tree(&self) -> &FenwickTree<i32> {
        &self.tree
    }

    /// Get the updated index for a position.
    ///
    /// Returns the prefix sum of the index + index.
    ///
    /// Panics if the input index does not fit within i32 or
    /// if the updated index does not fit within usize.
    pub fn transform(&self, index: usize) -> usize {
        usize::try_from(
            i32::try_from(index).expect("index fits into i32") + self.tree.prefix_sum(index, 0i32),
        )
        .expect("updated index fits into usize")
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::fix::Fixer;
    use crate::fix::InsertionPoint;
    use crate::fix::Replacement;

    #[test]
    fn test_fixer_insertion() {
        let value = String::from("hello");
        let insertion = String::from("world");
        let rep = Replacement::new(
            value.len(),
            value.len(),
            InsertionPoint::AfterEnd,
            insertion,
            2,
        );
        let rep2 = Replacement::new(5, 5, InsertionPoint::BeforeStart, String::from(" "), 1);

        let mut fixer = Fixer::new(value);
        let mut fixer2 = fixer.clone();

        fixer.apply_replacement(&rep);
        fixer.apply_replacement(&rep2);
        assert_eq!(fixer.value(), "hello world");

        fixer2.apply_replacements(vec![rep, rep2]);
        assert_eq!(fixer2.value(), "hello world");
    }

    #[test]
    fn test_fixer_deletion() {
        let value = String::from("My grammar is perfect.");
        let del = String::from("");
        let del2 = String::from("bad");
        let rep = Replacement::new(11, 14, InsertionPoint::BeforeStart, del, 2);
        let rep2 = Replacement::new(14, 21, InsertionPoint::AfterEnd, del2, 1);

        let mut fixer = Fixer::new(value);
        let mut fixer2 = fixer.clone();

        fixer.apply_replacement(&rep);
        fixer.apply_replacement(&rep2);
        assert_eq!(fixer.value(), "My grammar bad.");

        fixer2.apply_replacements(vec![rep2, rep]);
        assert_eq!(fixer2.value(), "My grammar bad.");
    }

    #[test]
    fn test_fixer_indel() {
        let value = String::from("This statement is false.");
        let del = String::from("");
        let ins = String::from("true");
        let rep = Replacement::new(18, 23, InsertionPoint::BeforeStart, del, 2);
        let rep2 = Replacement::new(18, 18, InsertionPoint::AfterEnd, ins, 1);

        let mut fixer = Fixer::new(value);
        let mut fixer2 = fixer.clone();

        fixer.apply_replacement(&rep);
        fixer.apply_replacement(&rep2);
        assert_eq!(fixer.value(), "This statement is true.");

        fixer2.apply_replacements(vec![rep2, rep]);
        assert_eq!(fixer2.value(), "This statement is true.");
    }

    #[test]
    #[should_panic]
    fn test_out_of_bounds_insert() {
        let value = String::from("012345");
        let ins = String::from("6");
        let rep = Replacement::new(7, 7, InsertionPoint::AfterEnd, ins, 1);

        let mut fixer = Fixer::new(value);
        fixer.apply_replacement(&rep);
    }
}

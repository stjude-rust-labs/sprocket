//! Lines within strings.

use std::num::NonZeroUsize;

/// Lines with offsets.
///
/// This struct is used as an iterator when iterating over lines with offsets.
#[derive(Debug)]
pub struct LinesWithOffsets<'a> {
    /// The remaining str slice.
    remaining: &'a str,

    /// The current offset within the str.
    current_offset: usize,

    /// The current line number.
    line_no: NonZeroUsize,
}

impl<'a> LinesWithOffsets<'a> {
    /// Creates a new [`LinesWithOffsets`].
    ///
    /// Note: you should not create this yourself. Instead, you should use the
    /// [`LinesWithOffsetsExt`] and the associated method on [`str`]s.
    fn new(input: &'a str) -> Self {
        LinesWithOffsets {
            remaining: input,
            current_offset: 0,
            // SAFETY: this will always unwrap as one is non-zero.
            line_no: NonZeroUsize::try_from(1).unwrap(),
        }
    }
}

impl<'a> Iterator for LinesWithOffsets<'a> {
    type Item = (NonZeroUsize, usize, usize, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }

        let (line_end, next_offset) = match self.remaining.find(|c| c == '\n' || c == '\r') {
            Some(idx) => {
                let next_offset = idx
                    + if self.remaining.as_bytes().get(idx) == Some(&b'\r')
                        && self.remaining.as_bytes().get(idx + 1) == Some(&b'\n')
                    {
                        2
                    } else {
                        1
                    };
                (idx, next_offset)
            }
            None => (self.remaining.len(), self.remaining.len()),
        };

        let line = &self.remaining[..line_end];
        let item = (
            self.line_no,
            self.current_offset,
            self.current_offset + line_end,
            line,
        );

        self.remaining = if next_offset < self.remaining.len() {
            &self.remaining[next_offset..]
        } else {
            ""
        };

        self.current_offset += next_offset;
        self.line_no = match self.line_no.checked_add(1) {
            Some(line_number) => line_number,
            // SAFETY: this will only occur if and when a file with
            // [`usize::MAX`] lines is passed through this iterator. Memory will
            // be exhausted before that happens, so we can effectively ignore
            // this error. We panic here for completeness.
            None => panic!("LinesWithOffsets does not support {} lines", usize::MAX),
        };

        Some(item)
    }
}

/// A utility trait for adding the enclosed methods to a [`str`].
pub trait LinesWithOffsetsExt {
    /// Calculates lines with the byte start offset, the byte end offset, and
    /// line numbers.
    fn lines_with_offsets(&self) -> LinesWithOffsets<'_>;
}

impl LinesWithOffsetsExt for str {
    fn lines_with_offsets(&self) -> LinesWithOffsets<'_> {
        LinesWithOffsets::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lines_with_offset() {
        let text = "Hello\nworld\r\nthis is a test";

        let mut iter = text.lines_with_offsets();
        assert_eq!(
            iter.next(),
            Some((NonZeroUsize::try_from(1).unwrap(), 0, 5, "Hello"))
        );
        assert_eq!(
            iter.next(),
            Some((NonZeroUsize::try_from(2).unwrap(), 6, 11, "world"))
        );
        assert_eq!(
            iter.next(),
            Some((NonZeroUsize::try_from(3).unwrap(), 13, 27, "this is a test"))
        );
        assert_eq!(iter.next(), None); // Ensure there are no more lines
    }
}

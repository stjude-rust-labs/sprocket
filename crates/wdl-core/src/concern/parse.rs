//! Concerns related to parsing.

use std::collections::HashSet;

use lazy_static::lazy_static;
use pest::error::ErrorVariant;
use pest::error::InputLocation;
use pest::RuleType;
use serde::Deserialize;
use serde::Serialize;

use crate::file::location::Position;
use crate::file::Location;

lazy_static! {
    /// The tokens to ignore when reporting out parse errors that have positive
    /// rules missing. These are generally considered as noise, as they are implied
    /// to be possible at (nearly) any token location.
    static ref IGNORED_POSITIVE_TOKENS_SET: HashSet<&'static str> = {
        let mut set = HashSet::new();
        set.insert("comment");
        set.insert("whitespace");
        set
    };

    /// The tokens to ignore when reporting out parse errors that have negatives
    /// rules missing. At the time of writing, there are no ignored negative
    /// tokens, but this may change in the future.
    static ref IGNORED_NEGATIVE_TOKENS_SET: HashSet<&'static str> = {
        HashSet::new()
    };
}

/// A parse error.
///
/// This is a wrapper type for a Pest [`Error`](pest::error::Error) to make it
/// easier to work with and report within Sprocket.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Error {
    /// The concern message.
    message: String,

    /// The location.
    location: Location,
}

impl Error {
    /// Creates a new parse [`Error`].
    ///
    /// Note: almost always, you should prefer converting from a [Pest
    /// error](pest::error::Error) to an [`Error`] using [`Error::from()`].
    /// Don't use this function unless you know what you are doing!
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::parse;
    /// use wdl_core::file::Location;
    ///
    /// let error = parse::Error::new("Hello, world!", Location::Unplaced);
    /// ```
    pub fn new(message: impl Into<String>, location: Location) -> Self {
        let message = message.into();

        Self { message, location }
    }

    /// Gets the message from the [`Error`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::parse;
    /// use wdl_core::file::Location;
    ///
    /// let error = parse::Error::new("Hello, world!", Location::Unplaced);
    /// assert_eq!(error.message(), "Hello, world!");
    /// ```
    pub fn message(&self) -> &str {
        self.message.as_ref()
    }

    /// Consumes `self` and returns the message for this [`Error`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::parse;
    /// use wdl_core::file::Location;
    ///
    /// let error = parse::Error::new("Hello, world!", Location::Unplaced);
    /// assert_eq!(error.into_message(), String::from("Hello, world!"));
    /// ```
    pub fn into_message(self) -> String {
        self.message
    }

    /// Gets the byte range from the [`Error`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::parse;
    /// use wdl_core::file::Location;
    ///
    /// let error = parse::Error::new("Hello, world!", Location::Unplaced);
    /// assert_eq!(error.byte_range(), None);
    /// ```
    pub fn byte_range(&self) -> Option<std::ops::Range<usize>> {
        match &self.location {
            Location::Unplaced => None,
            Location::Position(position) => Some(position.byte_no()..position.byte_no()),
            Location::Span { start, end } => Some(start.byte_no()..end.byte_no()),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;

        if self.location != Location::Unplaced {
            write!(f, " ({})", self.location)?;
        }

        Ok(())
    }
}

impl<R: RuleType> From<pest::error::Error<R>> for Error {
    fn from(err: pest::error::Error<R>) -> Self {
        let (start_byte_no, end_byte_no) = match err.location {
            InputLocation::Pos(pos) => (pos, pos),
            InputLocation::Span((start, end)) => (start, end),
        };

        let location = match err.line_col {
            pest::error::LineColLocation::Pos((line_no, col_no)) => Location::Position(
                Position::try_new(line_no, col_no, start_byte_no)
                    .expect("Pest should return line and column numbers that are one or greater"),
            ),
            pest::error::LineColLocation::Span(start, end) => Location::Span {
                start: Position::try_new(start.0, start.1, start_byte_no)
                    .expect("Pest should return line and column numbers that are one or greater"),
                end: Position::try_new(end.0, end.1, end_byte_no)
                    .expect("Pest should return line and column numbers that are one or greater"),
            },
        };

        let message = match err.variant {
            ErrorVariant::ParsingError {
                positives,
                negatives,
            } => {
                let mut parts = Vec::new();

                let positives = filter(positives, &IGNORED_POSITIVE_TOKENS_SET).join(", ");

                if !positives.is_empty() {
                    parts.push(format!("The following tokens are required: {}.", positives));
                }

                let negatives = filter(negatives, &IGNORED_NEGATIVE_TOKENS_SET).join(", ");

                if !negatives.is_empty() {
                    parts.push(format!(
                        "The following tokens are not allowed: {}.",
                        negatives
                    ));
                }

                if parts.is_empty() {
                    // SAFETY: Pest is always expected to return either
                    // positive or negative rulesets. If neither are
                    // returned, this case should be further
                    // investigated.
                    panic!("Pest should return either a positive or negative ruleset")
                }

                parts.join(" ")
            }
            ErrorVariant::CustomError { message } => message,
        };

        Error { message, location }
    }
}

/// Filters a set of rules
fn filter<R: RuleType>(rules: Vec<R>, ignored: &HashSet<&str>) -> Vec<String> {
    let rules = rules
        .into_iter()
        .map(|rule| format!("{:?}", rule).to_lowercase())
        .collect::<Vec<_>>();

    if rules.iter().all(|p| ignored.contains(p.as_str())) {
        return rules;
    }

    rules
        .into_iter()
        .filter(|p| !ignored.contains(p.as_str()))
        .collect()
}

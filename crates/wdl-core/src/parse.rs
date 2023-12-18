//! Generalized parse results.

use crate::concern::Concerns;

/// An error related to a parse [`Result`].
#[derive(Debug)]
pub enum Error {
    /// A contradiction was encountered.
    Contradiction(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Contradiction(reason) => {
                write!(f, "contradiction encountered: {reason}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// A parse result.
///
/// This struct contains the results of parsing either (a) a WDL parse tree or
/// (b) a WDL abstract syntax tree. It contains two distinct entities:
///
/// * An optional list of [`Concerns`], if any were emitted during parsing.
/// * An optional tree based on type `E`, if one was able to be constructed.
///
/// Notably, you cannot create a [`Result`] that has no concerns and no parse
/// tree, as that scenario is non-sensical.
#[derive(Debug)]
pub struct Result<E> {
    /// Concerns emitted during parsing (if there were any).
    concerns: Option<Concerns>,

    /// The inner parse tree (if one was able to be created).
    tree: Option<E>,
}

impl<E> Result<E> {
    /// Attempts to create a new [`Result`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::concerns::Builder;
    /// use wdl_core::concern::parse;
    /// use wdl_core::file::Location;
    /// use wdl_core::parse::Result;
    /// use wdl_core::Concern;
    ///
    /// // Substitute `42` for your parse tree or abstract syntax tree.
    /// let result = Result::try_new(Some(42), None).unwrap();
    /// assert!(result.tree().is_some());
    /// assert!(result.concerns().is_none());
    ///
    /// let error = parse::Error::new("Hello, world!", Location::Unplaced);
    /// let concern = Concern::ParseError(error);
    /// let concerns = Builder::default().push(concern).build();
    ///
    /// let result = Result::<usize>::try_new(None, concerns).unwrap();
    /// assert!(result.tree().is_none());
    /// assert!(result.concerns().is_some());
    ///
    /// let err = Result::<usize>::try_new(None, None).unwrap_err();
    /// assert_eq!(
    ///     err.to_string(),
    ///     String::from(
    ///         "contradiction encountered: cannot create a parse Result with no concerns and no \
    ///          parse tree"
    ///     )
    /// );
    /// ```
    pub fn try_new(
        tree: Option<E>,
        concerns: Option<Concerns>,
    ) -> std::result::Result<Self, Error> {
        if concerns.is_none() && tree.is_none() {
            return Err(Error::Contradiction(String::from(
                "cannot create a parse Result with no concerns and no parse tree",
            )));
        }

        Ok(Self { concerns, tree })
    }

    /// Gets the concerns from the [`Result`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::concerns::Builder;
    /// use wdl_core::concern::parse;
    /// use wdl_core::file::Location;
    /// use wdl_core::parse::Result;
    /// use wdl_core::Concern;
    ///
    /// let error = parse::Error::new("Hello, world!", Location::Unplaced);
    /// let concern = Concern::ParseError(error);
    /// let concerns = Builder::default().push(concern).build();
    ///
    /// let result = Result::<usize>::try_new(None, concerns).unwrap();
    ///
    /// let first = result.concerns().unwrap().inner().iter().next().unwrap();
    /// let error = first.as_parse_error().unwrap();
    /// assert_eq!(error.message(), "Hello, world!");
    /// ```
    pub fn concerns(&self) -> Option<&Concerns> {
        self.concerns.as_ref()
    }

    /// Consumes `self` and returns the concerns from the [`Result`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::concerns::Builder;
    /// use wdl_core::concern::parse;
    /// use wdl_core::file::Location;
    /// use wdl_core::parse::Result;
    /// use wdl_core::Concern;
    ///
    /// let error = parse::Error::new("Hello, world!", Location::Unplaced);
    /// let concern = Concern::ParseError(error);
    /// let concerns = Builder::default().push(concern).build();
    ///
    /// let result = Result::<usize>::try_new(None, concerns).unwrap();
    ///
    /// let first = result
    ///     .into_concerns()
    ///     .unwrap()
    ///     .into_inner()
    ///     .into_iter()
    ///     .next()
    ///     .unwrap();
    /// let error = first.into_parse_error().unwrap();
    /// assert_eq!(error.message(), String::from("Hello, world!"));
    /// ```
    pub fn into_concerns(self) -> Option<Concerns> {
        self.concerns
    }

    /// Gets the tree from the [`Result`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::parse::Result;
    ///
    /// // Substitute `42` for your parse tree or abstract syntax tree.
    /// let result = Result::<usize>::try_new(Some(42), None).unwrap();
    ///
    /// let first = result.tree().unwrap();
    /// assert_eq!(first, &42);
    /// ```
    pub fn tree(&self) -> Option<&E> {
        self.tree.as_ref()
    }

    /// Consumes `self` and returns the tree from the [`Result`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::parse::Result;
    ///
    /// // Substitute `42` for your parse tree or abstract syntax tree.
    /// let result = Result::<usize>::try_new(Some(42), None).unwrap();
    ///
    /// let first = result.into_tree().unwrap();
    /// assert_eq!(first, 42);
    /// ```
    pub fn into_tree(self) -> Option<E> {
        self.tree
    }

    /// Breaks a [`Result`] down into its parts.
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::concerns::Builder;
    /// use wdl_core::concern::parse;
    /// use wdl_core::file::Location;
    /// use wdl_core::parse::Result;
    /// use wdl_core::Concern;
    ///
    /// let error = parse::Error::new("Hello, world!", Location::Unplaced);
    /// let concern = Concern::ParseError(error);
    /// let concerns = Builder::default().push(concern).build();
    ///
    /// // Substitute `42` for your parse tree or abstract syntax tree.
    /// let result = Result::<usize>::try_new(Some(42), concerns).unwrap();
    /// assert_eq!(result.tree(), Some(&42));
    /// assert_eq!(
    ///     result
    ///         .concerns()
    ///         .unwrap()
    ///         .inner()
    ///         .iter()
    ///         .next()
    ///         .unwrap()
    ///         .as_parse_error()
    ///         .unwrap()
    ///         .message(),
    ///     "Hello, world!"
    /// );
    /// ```
    pub fn into_parts(self) -> (Option<E>, Option<Concerns>) {
        (self.tree, self.concerns)
    }
}

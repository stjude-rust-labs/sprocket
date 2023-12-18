//! Builder for [`Concerns`].

use nonempty::NonEmpty;

use crate::concern::Concern;
use crate::concern::Concerns;

/// A builder for [`Concerns`].
#[derive(Debug, Default)]
pub struct Builder(Option<NonEmpty<Concern>>);

impl Builder {
    /// Pushes a [`Concern`] into the [`Builder`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::concerns::Builder;
    /// use wdl_core::concern::parse;
    /// use wdl_core::file::Location;
    /// use wdl_core::Concern;
    ///
    /// let error = parse::Error::new("Hello, world!", Location::Unplaced);
    /// let concern = Concern::ParseError(error);
    /// let concerns = Builder::default().push(concern).build();
    ///
    /// assert!(concerns.is_some());
    /// ```
    pub fn push(mut self, concern: Concern) -> Self {
        let concerns = match self.0 {
            Some(mut concerns) => {
                concerns.push(concern);
                concerns
            }
            None => NonEmpty::new(concern),
        };

        self.0 = Some(concerns);
        self
    }

    /// Consumes self to build an [`Option<Concerns>`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::concern::concerns::Builder;
    /// use wdl_core::concern::parse;
    /// use wdl_core::file::Location;
    /// use wdl_core::Concern;
    ///
    /// let concerns = Builder::default().build();
    /// assert!(concerns.is_none());
    ///
    /// let error = parse::Error::new("Hello, world!", Location::Unplaced);
    /// let concern = Concern::ParseError(error);
    /// let concerns = Builder::default().push(concern).build();
    ///
    /// assert!(concerns.is_some());
    /// ```
    pub fn build(self) -> Option<Concerns> {
        let inner = self.0?;
        Some(Concerns::from(inner))
    }
}

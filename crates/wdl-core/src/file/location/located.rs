//! Located entities.

use super::Location;

/// A located entity.
///
/// See the [module documentation](crate::file::Location) for more information.
#[derive(Clone)]
pub struct Located<E> {
    /// The inner entity `E`.
    inner: E,

    /// The location.
    location: Location,
}

impl<E> Located<E> {
    /// Creates a new [`Located`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::file::location::Located;
    /// use wdl_core::file::Location;
    ///
    /// let located = Located::new(0usize, Location::Unplaced);
    /// assert_eq!(located.into_parts(), (0usize, Location::Unplaced));
    /// ```
    pub fn new(inner: E, location: Location) -> Self {
        Located { inner, location }
    }

    /// Creates a [`Located`] with the [`Location`] as [`Location::Unplaced`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::file::location::Located;
    /// use wdl_core::file::Location;
    ///
    /// let located = Located::unplaced(0usize);
    /// assert_eq!(located.into_parts(), (0usize, Location::Unplaced));
    /// ```
    pub fn unplaced(inner: E) -> Self {
        Located {
            inner,
            location: Location::Unplaced,
        }
    }

    /// Returns the inner `E` for the [`Located<E>`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::file::location::Located;
    /// use wdl_core::file::Location;
    ///
    /// let located = Located::new(0usize, Location::Unplaced);
    /// assert_eq!(located.inner(), &0usize);
    pub fn inner(&self) -> &E {
        &self.inner
    }

    /// Consumes `self` and returns the inner `E` for the [`Located<E>`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::file::location::Located;
    /// use wdl_core::file::Location;
    ///
    /// let located = Located::new(0usize, Location::Unplaced);
    /// assert_eq!(located.into_inner(), 0usize);
    /// ```
    pub fn into_inner(self) -> E {
        self.inner
    }

    /// Returns the [`Location`] for the [`Located<E>`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::file::location::Located;
    /// use wdl_core::file::Location;
    ///
    /// let located = Located::new(0usize, Location::Unplaced);
    /// assert_eq!(located.location(), &Location::Unplaced);
    /// ```
    pub fn location(&self) -> &Location {
        &self.location
    }

    /// Consumes `self` and returns the [`Location`] for the [`Located<E>`].
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::file::location::Located;
    /// use wdl_core::file::Location;
    ///
    /// let located = Located::new(0usize, Location::Unplaced);
    /// assert_eq!(located.into_location(), Location::Unplaced);
    /// ```
    pub fn into_location(self) -> Location {
        self.location
    }

    /// Consumes `self` to split the [`Located<E>`] into its respective parts.
    ///
    /// # Examples
    ///
    /// ```
    /// use wdl_core::file::location::Located;
    /// use wdl_core::file::Location;
    ///
    /// let located = Located::new(0usize, Location::Unplaced);
    /// assert_eq!(located.into_parts(), (0usize, Location::Unplaced));
    /// ```
    pub fn into_parts(self) -> (E, Location) {
        (self.inner, self.location)
    }

    /// Maps the inner value from an `F` to a `T`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::num::NonZeroUsize;
    ///
    /// use wdl_core::file::location::Located;
    /// use wdl_core::file::Location;
    ///
    /// let before = Located::new(1, Location::Unplaced);
    /// let after = before.map(|i| NonZeroUsize::try_from(i).unwrap());
    /// assert_eq!(after.into_inner(), NonZeroUsize::try_from(1).unwrap());
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn map<F, T>(self, f: F) -> Located<T>
    where
        F: FnOnce(E) -> T,
    {
        Located {
            inner: f(self.inner),
            location: self.location,
        }
    }
}

//=======//
// START //
//=======//

// Note:: within this block, the included trait implementations explicitly only
// consider the inner value. That fact that the locations are non-important and
// are only accessible when [`location()`] is explicitly called on a [`Located`]
// is by design.

impl<E> std::fmt::Display for Located<E>
where
    E: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl<E> std::fmt::Debug for Located<E>
where
    E: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}

impl<E> PartialEq for Located<E>
where
    E: std::cmp::PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<E> Eq for Located<E> where E: std::cmp::Eq {}

impl<E> PartialOrd for Located<E>
where
    E: std::cmp::PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.inner.partial_cmp(&other.inner)
    }
}

impl<E> Ord for Located<E>
where
    E: std::cmp::Ord,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.inner.cmp(&other.inner)
    }
}

impl<E> std::hash::Hash for Located<E>
where
    E: std::hash::Hash,
{
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}

//=====//
// END //
//=====//

impl<E> std::ops::Deref for Located<E> {
    type Target = E;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<E> std::borrow::Borrow<E> for Located<E> {
    fn borrow(&self) -> &E {
        &self.inner
    }
}

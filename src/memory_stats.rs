//! Peak memory usage reporting.
//!
//! The [`peak_alloc::PeakAlloc`] allocator is installed as the global
//! allocator, and queried when [`MemoryStatsGuard`] is dropped to report the
//! peak memory usage of the application.
//!
//! This module should only be included in binary targets (as opposed to
//! libraries), as there can be only one `global_allocator` set per compilation
//! target.

#[global_allocator]
static PEAK_ALLOC: peak_alloc::PeakAlloc = peak_alloc::PeakAlloc;

/// A guard value which, when dropped, reports the peak memory usage of the
/// process.
///
/// Using a guard is not strictly necessary, but helps prevent unexpected early
/// returns or future cancelations from interfering with the creation of this
/// output.
pub struct MemoryStatsGuard;

impl Drop for MemoryStatsGuard {
    fn drop(&mut self) {
        tracing::debug!(
            "peak memory usage {:.02} MiB",
            PEAK_ALLOC.peak_usage_as_mb()
        );
    }
}

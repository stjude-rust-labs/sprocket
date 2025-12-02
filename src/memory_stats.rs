//! Peak memory usage reporting.
//!
//! This module exists in all configurations, but does not do anything unless
//! the `memory_stats` Cargo feature is enabled. Under this configuration, the
//! [`peak_alloc::PeakAlloc`] allocator is installed as the global allocator,
//! and queried when [`MemoryStatsGuard`] is dropped to report the peak memory
//! usage of the application.

#[cfg(feature = "memory_stats")]
#[global_allocator]
static PEAK_ALLOC: peak_alloc::PeakAlloc = peak_alloc::PeakAlloc;

/// A guard value which, when dropped, reports the peak memory usage of the
/// process.
pub struct MemoryStatsGuard;

impl Drop for MemoryStatsGuard {
    fn drop(&mut self) {
        #[cfg(feature = "memory_stats")]
        tracing::info!(
            "peak memory usage {:.02} MiB",
            PEAK_ALLOC.peak_usage_as_mb()
        );
    }
}

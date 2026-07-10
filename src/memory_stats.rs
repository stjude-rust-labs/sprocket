//! Peak memory usage reporting.
//!
//! Peak resident set size is queried from the operating system when
//! [`MemoryStatsGuard`] is dropped. On Unix this uses `getrusage` and on
//! Windows it uses `GetProcessMemoryInfo`.

/// A guard value which, when dropped, reports the peak memory usage of the
/// process.
///
/// Using a guard is not strictly necessary, but helps prevent unexpected early
/// returns or future cancellations from interfering with the creation of this
/// output.
pub struct MemoryStatsGuard;

impl Drop for MemoryStatsGuard {
    fn drop(&mut self) {
        if let Some(bytes) = peak_memory_bytes() {
            tracing::debug!(
                "peak memory usage {:.02} MiB",
                bytes as f64 / (1024.0 * 1024.0)
            );
        }
    }
}

/// Returns the peak resident set size of the current process in bytes.
///
/// Returns `None` if the value could not be determined on the current platform.
#[cfg(unix)]
fn peak_memory_bytes() -> Option<u64> {
    // SAFETY: `getrusage` only writes to the `rusage` value pointed to by the
    // second argument and does not retain the pointer; a zeroed `rusage` is a
    // valid initial value. The return value is checked before the struct is
    // read.
    // Ref: https://man7.org/linux/man-pages/man2/getrusage.2.html
    let max_rss = unsafe {
        let mut usage = std::mem::zeroed::<libc::rusage>();
        if libc::getrusage(libc::RUSAGE_SELF, &mut usage) != 0 {
            return None;
        }
        usage.ru_maxrss as u64
    };

    // `ru_maxrss` is reported in bytes on macOS but in kibibytes on Linux and
    // the BSDs.
    // Ref (Linux, KiB): https://man7.org/linux/man-pages/man2/getrusage.2.html
    // Ref (macOS, bytes): https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/getrusage.2.html
    #[cfg(target_os = "macos")]
    {
        Some(max_rss)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Some(max_rss * 1024)
    }
}

/// Returns the peak resident set size of the current process in bytes.
///
/// Returns `None` if the value could not be determined on the current platform.
#[cfg(windows)]
fn peak_memory_bytes() -> Option<u64> {
    use windows_sys::Win32::System::ProcessStatus::GetProcessMemoryInfo;
    use windows_sys::Win32::System::ProcessStatus::PROCESS_MEMORY_COUNTERS;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    // SAFETY: `GetProcessMemoryInfo` writes up to `cb` bytes into the counters
    // value pointed to by the second argument; `cb` is the size of the value we
    // allocate here. The current-process pseudo-handle is always valid, and the
    // counters are only read after the call reports success (a nonzero return).
    // Ref: https://learn.microsoft.com/en-us/windows/win32/api/psapi/nf-psapi-getprocessmemoryinfo
    // Ref: https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getcurrentprocess
    unsafe {
        let mut counters = PROCESS_MEMORY_COUNTERS::default();
        let cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, cb) == 0 {
            return None;
        }
        Some(counters.PeakWorkingSetSize as u64)
    }
}

/// Returns the peak resident set size of the current process in bytes.
///
/// Returns `None` if the value could not be determined on the current platform.
#[cfg(not(any(unix, windows)))]
fn peak_memory_bytes() -> Option<u64> {
    None
}

//! The Sprocket command line binary.

#![allow(missing_docs)]
#![allow(clippy::missing_docs_in_private_items)]

mod memory_stats;

/// The allocation-heavy, multithreaded parse and analysis workload benefits
/// measurably from mimalloc over the system allocator.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() {
    sprocket::sprocket_main(memory_stats::MemoryStatsGuard).await
}

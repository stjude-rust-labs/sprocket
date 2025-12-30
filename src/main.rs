//! The Sprocket command line binary.

#![allow(missing_docs)]
#![allow(clippy::missing_docs_in_private_items)]

mod memory_stats;

#[tokio::main]
async fn main() {
    sprocket::sprocket_main(memory_stats::MemoryStatsGuard).await
}

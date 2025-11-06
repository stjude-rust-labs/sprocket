//! The Sprocket command line binary.

#![allow(missing_docs)]
#![allow(clippy::missing_docs_in_private_items)]

#[tokio::main]
pub async fn main() {
    sprocket::sprocket_main().await
}

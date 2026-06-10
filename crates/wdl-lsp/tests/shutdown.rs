//! Integration tests for the `shutdown` request.

use async_lsp::lsp_types::request::Shutdown;

use crate::common::TestContext;

pub mod common;

#[tokio::test]
async fn should_shutdown_without_error() {
    let ctx = TestContext::new("baseline");
    ctx.request::<Shutdown>(()).await.unwrap();
}

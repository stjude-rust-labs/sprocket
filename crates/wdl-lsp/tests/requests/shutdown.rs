//! Integration tests for the `shutdown` request.

use async_lsp::lsp_types::request::Shutdown;

use crate::common::TestContextBuilder;

#[tokio::test]
async fn should_shutdown_without_error() {
    let ctx = TestContextBuilder::new("baseline").build();
    ctx.request::<Shutdown>(()).await.unwrap();
}

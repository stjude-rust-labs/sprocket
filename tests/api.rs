//! API end-to-end tests.

/// Determines if Docker tests are enabled.
fn docker_tests_enabled() -> bool {
    std::env::var("DISABLE_DOCKER_TESTS").is_err()
}

#[path = "api/runs.rs"]
mod runs;

#[path = "api/sessions.rs"]
mod sessions;

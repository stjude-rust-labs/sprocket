//! Shared fixtures and helpers for the `sprocket dev module` integration tests.

#[path = "fixtures/assertions.rs"]
mod assertions;
#[path = "fixtures/command.rs"]
mod command;
#[path = "fixtures/git.rs"]
mod git;
#[path = "fixtures/module.rs"]
mod module;
#[path = "fixtures/signer.rs"]
mod signer;

pub(crate) use assertions::*;
pub(crate) use command::*;
pub(crate) use git::*;
pub(crate) use module::*;
pub(crate) use signer::*;

#[test]
fn focused_fixture_modules_preserve_the_test_api() {
    let _ = sprocket as fn(&[&str]) -> std::process::Command;
    let _ = commit as fn(&git2::Repository, &str);
    let _ = read_lockfile as fn(&std::path::Path) -> wdl_modules::Lockfile;
    fn consume<T>() {}
    consume::<ModuleFixture>();
    consume::<GitFixture>();
    consume::<SignerScenario>();
}

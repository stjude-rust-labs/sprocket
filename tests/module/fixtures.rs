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

//! Integration tests for `sprocket dev module` commands.

#[path = "module/fixtures.rs"]
mod fixtures;

#[path = "module/init.rs"]
mod init;

#[path = "module/add_remove.rs"]
mod add_remove;

#[path = "module/lock.rs"]
mod lock;

#[path = "module/update_upgrade.rs"]
mod update_upgrade;

#[path = "module/trust.rs"]
mod trust;

#[path = "module/sign_verify.rs"]
mod sign_verify;

#[path = "module/tree_fetch_cache.rs"]
mod tree_fetch_cache;

#[path = "module/run.rs"]
mod run;

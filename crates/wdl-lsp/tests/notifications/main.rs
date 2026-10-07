//! Integration tests for the LSP notifications.

mod did_change_configuration;
mod did_change_watched_files;
mod did_change_workspace_folders;
mod did_create_files;
mod did_delete_files;
mod did_rename_files;

#[path = "../common.rs"]
mod common;

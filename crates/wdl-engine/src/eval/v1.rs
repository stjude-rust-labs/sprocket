//! Implementation of evaluation for V1 documents.

mod expr;
mod task;
mod workflow;

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
pub use expr::*;
use serde::Serialize;
pub use task::*;
pub use workflow::*;

/// The name of the inputs file to write for each task and workflow in the
/// outputs directory.
const INPUTS_FILE: &str = "inputs.json";

/// The name of the outputs file to write for each task and workflow in the
/// outputs directory.
const OUTPUTS_FILE: &str = "outputs.json";

/// Serializes a value into a JSON file.
fn write_json_file(path: impl AsRef<Path>, value: &impl Serialize) -> Result<()> {
    let path = path.as_ref();
    let file = File::create(path)
        .with_context(|| format!("failed to create file `{path}`", path = path.display()))?;
    serde_json::to_writer_pretty(BufWriter::new(file), value)
        .with_context(|| format!("failed to write file `{path}`", path = path.display()))
}

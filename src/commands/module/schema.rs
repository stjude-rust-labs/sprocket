use anyhow::Context;
use wdl_modules::Manifest;

use crate::commands::CommandOutput;
use crate::commands::CommandResult;

pub fn schema(output: CommandOutput) -> CommandResult<()> {
    let schema = schemars::schema_for!(Manifest);
    let schema_pretty =
        serde_json::to_string_pretty(&schema).context("serializing module schema")?;
    output.payload(schema_pretty);
    Ok(())
}

use pest_derive::Parser;

#[cfg(test)]
mod tests;

#[derive(Debug, Parser)]
#[grammar = "v1/wdl.pest"]
pub struct Parser;

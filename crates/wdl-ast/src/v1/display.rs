//! Provides rich documentation for WDL definitions used by LSP hovers and
//! completions.
//!
//! TODO: This functionality can be shared with `wdl-doc` ? It might make
//! sense to fold this into wdl-analysis maybe?

use std::fmt::{self};

use crate::AstNode;
use crate::AstToken;
use crate::TreeNode;
use crate::v1::InputSection;
use crate::v1::MetadataArray;
use crate::v1::MetadataObjectItem;
use crate::v1::MetadataValue;
use crate::v1::OutputSection;
use crate::v1::ParameterMetadataSection;

/// Formats a metadata value.
pub fn format_meta_value<N: TreeNode>(
    f: &mut impl fmt::Write,
    value: &MetadataValue<N>,
    indent: usize,
) -> fmt::Result {
    let prefix = " ".repeat(indent);
    match value {
        MetadataValue::Boolean(b) => writeln!(f, "{prefix}- `{}`", b.value()),
        MetadataValue::Integer(i) => writeln!(f, "{prefix}- `{}`", i.value().unwrap_or(0)),
        MetadataValue::Float(fl) => {
            writeln!(f, "{prefix}- `{}`", fl.value().unwrap_or(0.0))
        }
        MetadataValue::String(s) => {
            if let Some(text) = s.text() {
                writeln!(f, "{prefix}- `{}`", text.text())?
            }
            Ok(())
        }
        MetadataValue::Null(_) => writeln!(f, "{prefix}- `null`"),
        MetadataValue::Object(obj) => write_meta_object(f, obj.items(), indent),
        MetadataValue::Array(arr) => write_meta_array(f, arr, indent),
    }
}

/// Formats a metadata object.
pub fn write_meta_object<N: TreeNode, Items: Iterator<Item = MetadataObjectItem<N>>>(
    f: &mut impl fmt::Write,
    items: Items,
    indent: usize,
) -> fmt::Result {
    let prefix = " ".repeat(indent);
    for item in items {
        write!(f, "{prefix}- **{}**", item.name().text())?;
        format_meta_value(f, &item.value(), indent + 2)?;
    }
    Ok(())
}

/// Formats a metadata array.
fn write_meta_array<N: TreeNode>(
    f: &mut impl fmt::Write,
    arr: &MetadataArray<N>,
    indent: usize,
) -> fmt::Result {
    for value in arr.elements() {
        format_meta_value(f, &value, indent)?;
    }
    Ok(())
}

/// Gets the entire metadata value for a given parameter name.
pub fn get_param_meta<N: TreeNode>(
    name: &str,
    param_meta: Option<&ParameterMetadataSection<N>>,
) -> Option<MetadataValue<N>> {
    param_meta
        .and_then(|pm| pm.items().find(|i| i.name().text() == name))
        .map(|item| item.value())
}

/// Formats the input section with parameter metadata.
pub fn write_input_section<N: TreeNode>(
    f: &mut impl fmt::Write,
    input: Option<&InputSection<N>>,
    param_meta: Option<&ParameterMetadataSection<N>>,
) -> fmt::Result {
    if let Some(input) = input
        && input.declarations().next().is_some()
    {
        writeln!(f, "\n**Inputs**")?;
        for decl in input.declarations() {
            let name = decl.name();
            let default = decl.expr().map(|e| e.text().to_string());

            write!(f, "- **{}**: `{}`", name.text(), decl.ty().inner().text())?;
            if let Some(val) = default {
                // default values
                write!(f, " = *`{}`*", val.trim_start_matches(" = "))?;
            }

            if let Some(meta_val) = get_param_meta(name.text(), param_meta) {
                writeln!(f)?;
                format_meta_value(f, &meta_val, 2)?;
                writeln!(f)?;
            } else {
                writeln!(f)?;
            }
        }
    }
    Ok(())
}

/// Formats the output section with parameter metadata.
pub fn write_output_section<N: TreeNode>(
    f: &mut impl fmt::Write,
    output: Option<&OutputSection<N>>,
    param_meta: Option<&ParameterMetadataSection<N>>,
) -> fmt::Result {
    if let Some(output) = output
        && output.declarations().next().is_some()
    {
        writeln!(f, "\n**Outputs**")?;
        for decl in output.declarations() {
            let name = decl.name();
            write!(f, "- **{}**: `{}`", name.text(), decl.ty().inner().text())?;
            if let Some(meta_val) = get_param_meta(name.text(), param_meta) {
                writeln!(f)?;
                format_meta_value(f, &meta_val, 2)?;
                writeln!(f)?;
            } else {
                writeln!(f)?;
            }
        }
    }
    Ok(())
}

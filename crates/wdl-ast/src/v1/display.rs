//! Provides rich documentation for WDL definitions used by LSP hovers and
//! completions.
//!
//! TODO: This functionality can be shared with `wdl-doc` ? It might make
//! sense to fold this into wdl-analysis maybe?

use std::fmt;

use crate::AstToken;
use crate::TreeNode;
use crate::v1::MetadataArray;
use crate::v1::MetadataObjectItem;
use crate::v1::MetadataValue;
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

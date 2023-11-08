use pest_derive::Parser;

#[cfg(test)]
mod tests;

#[derive(Debug, Parser)]
#[grammar = "v1/wdl.pest"]
pub struct Parser;

/// Gets a rule by name.
///
/// # Examples
///
/// ```
/// use wdl_grammar as wdl;
///
/// let rule = wdl::v1::get_rule("document");
/// assert!(matches!(rule, Some(_)));
///
/// let rule = wdl::v1::get_rule("foo-bar-baz-rule");
/// assert!(!matches!(rule, Some(_)));
/// ```
pub fn get_rule(rule: &str) -> Option<Rule> {
    for candidate in Rule::all_rules() {
        if format!("{:?}", candidate) == rule {
            return Some(*candidate);
        }
    }

    None
}

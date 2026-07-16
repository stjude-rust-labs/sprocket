//! Caches compiled regular expressions for WDL standard library functions.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::Mutex;

use regex::Regex;

/// Caches successfully compiled regular expressions by pattern.
static REGEX_CACHE: LazyLock<Mutex<HashMap<String, Regex>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Gets a cached regular expression or compiles and caches the given pattern.
pub(super) fn get_or_compile_regex(pattern: &str) -> Result<Regex, regex::Error> {
    let mut cache = REGEX_CACHE.lock().expect("failed to lock regex cache");
    if let Some(regex) = cache.get(pattern) {
        return Ok(regex.clone());
    }

    let regex = Regex::new(pattern)?;
    cache.insert(pattern.to_string(), regex.clone());
    Ok(regex)
}

#[cfg(test)]
mod test {
    use super::get_or_compile_regex;

    #[test]
    fn gets_or_compiles_regex() {
        let pattern = r"\d+";

        // SAFETY: the pattern is a valid regular expression.
        let regex = get_or_compile_regex(pattern).unwrap();
        assert_eq!(regex.as_str(), pattern);

        // SAFETY: the pattern is a valid regular expression.
        let cached = get_or_compile_regex(pattern).unwrap();
        assert_eq!(cached.as_str(), pattern);

        assert!(get_or_compile_regex("..\\").is_err());
    }
}

//! Implements regex memoization

use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::Mutex;

use regex::Regex;

/// Keeps track of the previously calculated Regexes
///
/// We only cache the sucessfully compiled Regex values
static REGEX_CACHE: LazyLock<Mutex<HashMap<String, Regex>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Function for memoization of computed Regex values
pub fn cached_regex(regex_pattern: &str) -> Result<Regex, regex::Error> {
    let mut locked_cache = REGEX_CACHE.lock().expect("failed to lock regex cache");
    let re = locked_cache.get(regex_pattern);
    if let Some(value) = re {
        return Ok(value.clone());
    }

    let new_reg_ex = Regex::new(regex_pattern);
    if let Ok(value) = &new_reg_ex {
        locked_cache.insert(regex_pattern.to_string(), value.clone());
    }
    new_reg_ex
}

#[cfg(test)]
mod test {
    use regex::Regex;

    use crate::stdlib::regex_cache::cached_regex;

    #[tokio::test]
    async fn regex_cache() {
        let re_string = r"\d+".to_string();
        {
            let re: Regex = cached_regex(&re_string).unwrap();
            assert_eq!(re.as_str(), re_string);
        }
        {
            let re_cached: Regex = cached_regex(&re_string).unwrap();
            assert_eq!(re_cached.as_str(), re_string);
        }
        {
            let err = cached_regex("..\\");
            assert!(err.is_err());
        }
    }
}

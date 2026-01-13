//! Implementation of utility functions for reading task hints.

use std::collections::HashMap;

use anyhow::Result;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::v1::TASK_HINT_CACHEABLE;
use wdl_ast::v1::TASK_HINT_MAX_CPU;
use wdl_ast::v1::TASK_HINT_MAX_CPU_ALIAS;
use wdl_ast::v1::TASK_HINT_MAX_MEMORY;
use wdl_ast::v1::TASK_HINT_MAX_MEMORY_ALIAS;

use crate::Coercible;
use crate::TaskInputs;
use crate::Value;
use crate::config::CallCachingMode;
use crate::config::Config;
use crate::v1::task::lookup_entry;
use crate::v1::task::parse_storage_value;
use crate::v1::validators::SettingSource;
use crate::v1::validators::ensure_non_negative_i64;
use crate::v1::validators::invalid_numeric_value_message;

/// Gets the `max_cpu` hint from a hints map.
pub(crate) fn max_cpu(inputs: &TaskInputs, hints: &HashMap<String, Value>) -> Option<f64> {
    lookup_entry(&[TASK_HINT_MAX_CPU, TASK_HINT_MAX_CPU_ALIAS], |key| {
        inputs.hint(key).or_else(|| hints.get(key))
    })
    .map(|(_, v)| {
        v.coerce(None, &PrimitiveType::Float.into())
            .expect("type should coerce")
            .unwrap_float()
    })
}

/// Gets the `max_memory` hint from a hints map.
pub(crate) fn max_memory(
    inputs: &TaskInputs,
    hints: &HashMap<String, Value>,
) -> Result<Option<i64>> {
    match lookup_entry(&[TASK_HINT_MAX_MEMORY, TASK_HINT_MAX_MEMORY_ALIAS], |key| {
        inputs.hint(key).or_else(|| hints.get(key))
    }) {
        Some((key, value)) => {
            let bytes = parse_storage_value(value, |raw| {
                invalid_numeric_value_message(SettingSource::Hint, key, raw)
            })?;
            ensure_non_negative_i64(SettingSource::Hint, key, bytes).map(Some)
        }
        None => Ok(None),
    }
}

/// Gets the `preemptible` hint from a hints map.
///
/// This hint is not part of the WDL standard but is used for compatibility with
/// Cromwell where backends can support preemptible retries before using
/// dedicated instances.
pub(crate) fn preemptible(inputs: &TaskInputs, hints: &HashMap<String, Value>) -> Result<i64> {
    const TASK_HINT_PREEMPTIBLE: &str = "preemptible";
    const DEFAULT_TASK_HINT_PREEMPTIBLE: i64 = 0;

    Ok(lookup_entry(&[TASK_HINT_PREEMPTIBLE], |key| {
        inputs.hint(key).or_else(|| hints.get(key))
    })
    .and_then(|(_, v)| {
        v.coerce(None, &PrimitiveType::Integer.into())
            .ok()
            .map(|value| value.unwrap_integer())
    })
    .map(|value| ensure_non_negative_i64(SettingSource::Hint, TASK_HINT_PREEMPTIBLE, value))
    .transpose()?
    .unwrap_or(DEFAULT_TASK_HINT_PREEMPTIBLE))
}

/// Gets the `cacheable` hint from a hints map with config fallback.
pub(crate) fn cacheable(
    inputs: &TaskInputs,
    hints: &HashMap<String, Value>,
    config: &Config,
) -> bool {
    lookup_entry(&[TASK_HINT_CACHEABLE], |key| {
        inputs.hint(key).or_else(|| hints.get(key))
    })
    .and_then(|(_, v)| v.as_boolean())
    .unwrap_or(match config.task.cache {
        CallCachingMode::Off | CallCachingMode::Explicit => false,
        CallCachingMode::On => true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preemptible_disallows_negative_values() {
        let mut hints = HashMap::new();
        hints.insert("preemptible".to_string(), Value::from(-3));
        let err = preemptible(&TaskInputs::default(), &hints)
            .expect_err("`preemptible` should reject negatives");
        assert!(
            err.to_string()
                .contains("task hint `preemptible` cannot be less than zero")
        );
    }
}

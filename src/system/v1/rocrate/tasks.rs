//! Maps v1 task records to step-level RO-Crate provenance entities.
//!
//! The v1 database stores one task row per WDL task name (the task monitor keys
//! tasks by name; Crankshaft ids, shards, and retry attempts are not persisted),
//! so provenance is emitted at name granularity: one tool/step/action/control
//! bundle per task row. True per-execution identity (shard/attempt) requires
//! extending the task monitor and schema and is left to a follow-on.

use std::collections::HashMap;

use chrono::DateTime;
use chrono::SecondsFormat;
use chrono::Utc;
use rocraters::ro_crate::constraints::DataType;
use rocraters::ro_crate::constraints::EntityValue;
use rocraters::ro_crate::constraints::Id;
use rocraters::ro_crate::contextual_entity::ContextualEntity;
use rocraters::ro_crate::graph_vector::GraphVector;

use super::value::sanitize_component;
use crate::system::v1::db::models::Task;
use crate::system::v1::db::models::TaskStatus;

/// The `@id`s of the entities emitted for one task.
#[derive(Debug, Clone)]
pub struct TaskEntityIds {
    /// The tool entity (`SoftwareApplication`) representing the WDL task.
    pub tool: String,
    /// The planned `HowToStep`.
    pub step: String,
    /// The tool-execution `CreateAction`.
    pub action: String,
    /// The `ControlAction` tying the step to the execution.
    pub control: String,
}

/// Builds a `dynamic_entity` map from `(key, value)` pairs.
fn bag(pairs: Vec<(&str, EntityValue)>) -> Option<HashMap<String, EntityValue>> {
    Some(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// Wraps an `@id` reference as an `EntityValue`.
fn ev_id(s: &str) -> EntityValue {
    EntityValue::EntityId(Id::Id(s.to_string()))
}

/// Wraps a string as an `EntityValue`.
fn ev_str(s: &str) -> EntityValue {
    EntityValue::EntityString(s.to_string())
}

/// Canonical `xsd:dateTime` form (`...Z`, no fractional seconds).
fn iso(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Maps a v1 task status onto a schema.org `ActionStatus` URI.
pub fn task_action_status(s: &TaskStatus) -> &'static str {
    match s {
        TaskStatus::Completed => "http://schema.org/CompletedActionStatus",
        TaskStatus::Failed | TaskStatus::Canceled | TaskStatus::Preempted => {
            "http://schema.org/FailedActionStatus"
        }
        TaskStatus::Running => "http://schema.org/ActiveActionStatus",
        TaskStatus::Pending => "http://schema.org/PotentialActionStatus",
    }
}

/// Appends the provenance entities for one task to `graph` and returns their
/// `@id`s. Per the Provenance Run Crate profile:
///
/// - a tool entity (`SoftwareApplication`) represents the WDL task;
/// - the `HowToStep` is the planned step and references the tool via
///   `workExample`;
/// - the `CreateAction` records the execution and references the tool via
///   `instrument`;
/// - the `ControlAction` ties the step (`instrument`) to the execution
///   (`object`).
///
/// Task input/output links are intentionally omitted: the database does not
/// record which concrete files a task consumed or produced, and the profile
/// forbids guessing them.
pub fn task_step_entities(
    task: &Task,
    position: usize,
    graph: &mut Vec<GraphVector>,
) -> TaskEntityIds {
    let slug = sanitize_component(&task.name);
    let tool_id = format!("#tool-{slug}");
    let step_id = format!("#step-{slug}");
    let action_id = format!("#task-{slug}");
    let control_id = format!("#control-{slug}");

    // Tool entity: the WDL task definition that was executed.
    graph.push(GraphVector::ContextualEntity(ContextualEntity {
        id: tool_id.clone(),
        type_: DataType::Term("SoftwareApplication".to_string()),
        dynamic_entity: bag(vec![
            ("name", ev_str(&task.name)),
            ("description", ev_str(&format!("WDL task `{}`", task.name))),
        ]),
    }));

    // Planned step.
    graph.push(GraphVector::ContextualEntity(ContextualEntity {
        id: step_id.clone(),
        type_: DataType::Term("HowToStep".to_string()),
        dynamic_entity: bag(vec![
            ("name", ev_str(&task.name)),
            ("position", EntityValue::Entityi64(position as i64)),
            ("workExample", ev_id(&tool_id)),
        ]),
    }));

    // Tool execution.
    let mut action_props = vec![
        ("name", ev_str(&task.name)),
        ("instrument", ev_id(&tool_id)),
        ("actionStatus", ev_id(task_action_status(&task.status))),
    ];
    if let Some(start) = task.started_at {
        action_props.push(("startTime", ev_str(&iso(start))));
    }
    if let Some(end) = task.completed_at {
        action_props.push(("endTime", ev_str(&iso(end))));
    }
    if let Some(code) = task.exit_status {
        action_props.push(("exitStatus", EntityValue::Entityi64(code as i64)));
    }
    if let Some(error) = task.error.as_deref().filter(|e| !e.is_empty()) {
        action_props.push(("error", ev_str(error)));
    }
    graph.push(GraphVector::ContextualEntity(ContextualEntity {
        id: action_id.clone(),
        type_: DataType::Term("CreateAction".to_string()),
        dynamic_entity: bag(action_props),
    }));

    // Step execution control: links the planned step to the execution.
    graph.push(GraphVector::ContextualEntity(ContextualEntity {
        id: control_id.clone(),
        type_: DataType::Term("ControlAction".to_string()),
        dynamic_entity: bag(vec![
            ("instrument", ev_id(&step_id)),
            ("object", ev_id(&action_id)),
        ]),
    }));

    TaskEntityIds {
        tool: tool_id,
        step: step_id,
        action: action_id,
        control: control_id,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn completed_task(name: &str, exit: i32) -> Task {
        let now = Utc::now();
        Task {
            name: name.to_string(),
            run_uuid: uuid::Uuid::new_v4(),
            status: TaskStatus::Completed,
            exit_status: Some(exit),
            error: None,
            created_at: now,
            started_at: Some(now),
            completed_at: Some(now),
        }
    }

    #[test]
    fn status_maps_to_schema_uris() {
        assert_eq!(
            task_action_status(&TaskStatus::Completed),
            "http://schema.org/CompletedActionStatus"
        );
        assert_eq!(
            task_action_status(&TaskStatus::Failed),
            "http://schema.org/FailedActionStatus"
        );
        assert_eq!(
            task_action_status(&TaskStatus::Preempted),
            "http://schema.org/FailedActionStatus"
        );
        assert_eq!(
            task_action_status(&TaskStatus::Running),
            "http://schema.org/ActiveActionStatus"
        );
    }

    #[test]
    fn task_produces_tool_step_action_and_control() {
        let task = completed_task("align", 0);
        let mut graph = Vec::new();
        let ids = task_step_entities(&task, 1, &mut graph);
        assert_eq!(ids.tool, "#tool-align");
        assert_eq!(ids.action, "#task-align");

        let json = serde_json::to_string(&graph).unwrap();
        assert!(json.contains("SoftwareApplication"));
        assert!(json.contains("HowToStep"));
        assert!(json.contains("ControlAction"));
        assert!(json.contains("workExample"));
        assert!(json.contains("CompletedActionStatus"));
        assert!(json.contains("exitStatus"));

        // The execution references the tool via `instrument`, the step via
        // `workExample`, and the control ties step to action.
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let action = v
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["@id"] == "#task-align")
            .unwrap();
        assert_eq!(action["instrument"]["@id"], "#tool-align");
    }
}

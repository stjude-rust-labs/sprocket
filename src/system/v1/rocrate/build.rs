//! Assembles the Workflow Run RO-Crate graph from a `RunCrateContext`.

use std::collections::HashMap;

use anyhow::Result;
use rocraters::ro_crate::constraints::DataType;
use rocraters::ro_crate::constraints::EntityValue;
use rocraters::ro_crate::constraints::Id;
use rocraters::ro_crate::constraints::License;
use rocraters::ro_crate::context::ContextItem;
use rocraters::ro_crate::contextual_entity::ContextualEntity;
use rocraters::ro_crate::data_entity::DataEntity;
use rocraters::ro_crate::graph_vector::GraphVector;
use rocraters::ro_crate::metadata_descriptor::MetadataDescriptor;
use rocraters::ro_crate::rocrate::RoCrate;
use rocraters::ro_crate::rocrate::RoCrateContext;
use rocraters::ro_crate::root::RootDataEntity;
use wdl::analysis::document::Input;
use wdl::analysis::document::Output;
use wdl::analysis::types::Optional;

use super::METADATA_FILE;
use super::PROFILES;
use super::ROCRATE_CONTEXT;
use super::RoCrateOptions;
use super::RunCrateContext;
use super::WFRUN_CONTEXT;
use super::WORKFLOW_ID;
use super::formal::formal_parameter;
use super::value::value_to_entities;
use crate::system::v1::exec::Target;

/// Wraps an `@id` reference as an `EntityValue`.
fn ev_id(s: &str) -> EntityValue {
    EntityValue::EntityId(Id::Id(s.to_string()))
}

/// Wraps a list of `@id` references as an `EntityValue`.
fn ev_ids(v: Vec<String>) -> EntityValue {
    EntityValue::EntityId(Id::IdArray(v))
}

/// Wraps a string literal as an `EntityValue`.
fn ev_str(s: &str) -> EntityValue {
    EntityValue::EntityString(s.to_string())
}

/// Builds the `dynamic_entity` map from `(key, value)` pairs.
fn bag(pairs: Vec<(&str, EntityValue)>) -> Option<HashMap<String, EntityValue>> {
    Some(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// The selected callable's input and output declarations, by name.
type CallableInterface<'a> = (Vec<(String, &'a Input)>, Vec<(String, &'a Output)>);

/// Returns the selected callable's input and output declarations.
fn callable_interface<'a>(ctx: &'a RunCrateContext<'a>) -> CallableInterface<'a> {
    match ctx.target {
        Target::Workflow(name) => {
            if let Some(wf) = ctx.document.workflow().filter(|w| w.name() == name) {
                return (
                    wf.inputs().iter().map(|(k, v)| (k.clone(), v)).collect(),
                    wf.outputs().iter().map(|(k, v)| (k.clone(), v)).collect(),
                );
            }
            (Vec::new(), Vec::new())
        }
        Target::Task(name) => {
            if let Some(task) = ctx.document.task_by_name(name) {
                return (
                    task.inputs().iter().map(|(k, v)| (k.clone(), v)).collect(),
                    task.outputs().iter().map(|(k, v)| (k.clone(), v)).collect(),
                );
            }
            (Vec::new(), Vec::new())
        }
    }
}

/// Builds the full Workflow Run RO-Crate graph for a completed run.
///
/// Localizing inputs/outputs can fail (e.g. an un-localizable remote value); the
/// error propagates so the caller can honor `--ro-crate-strict`.
pub fn build_run_crate(ctx: &RunCrateContext<'_>, opts: &RoCrateOptions) -> Result<RoCrate> {
    let crate_root = ctx.run_dir.root();
    let mut crate_ = RoCrate {
        context: RoCrateContext::ReferenceContext(ROCRATE_CONTEXT.to_string()),
        graph: Vec::new(),
    };
    crate_.add_context(ContextItem::ReferenceItem(WFRUN_CONTEXT.to_string()));

    // Task targets produce a Task Run Crate: a Workflow Run Crate whose main
    // executable entity is an implicit one-task workflow wrapping the task.
    let (wf_name, wf_desc) = match ctx.target {
        Target::Workflow(name) => (name.clone(), format!("WDL workflow `{name}`")),
        Target::Task(name) => (
            format!("{name} (implicit workflow)"),
            format!("Implicit one-task workflow wrapping WDL task `{name}`"),
        ),
    };
    let (inputs_iface, outputs_iface) = callable_interface(ctx);
    let input_param_ids = inputs_iface
        .iter()
        .map(|(name, _)| format!("#param-in-{name}"))
        .collect::<Vec<_>>();
    let output_param_ids = outputs_iface
        .iter()
        .map(|(name, _)| format!("#param-out-{name}"))
        .collect::<Vec<_>>();
    let input_params = inputs_iface
        .iter()
        .map(|(name, _)| (name.clone(), format!("#param-in-{name}")))
        .collect::<HashMap<_, _>>();
    let output_params = outputs_iface
        .iter()
        .map(|(name, _)| (name.clone(), format!("#param-out-{name}")))
        .collect::<HashMap<_, _>>();

    // Timing is recorded only when actually known; `startTime`/`endTime` are not
    // required, so we omit them rather than fabricate values.
    let started = ctx.run.started_at.map(|t| t.to_rfc3339());
    let ended = ctx.run.completed_at.map(|t| t.to_rfc3339());
    // `date_published` is required on the root dataset; use a real run timestamp
    // (start when known, else the creation time) rather than inventing one.
    let date_published = ctx
        .run
        .started_at
        .unwrap_or(ctx.run.created_at)
        .to_rfc3339();

    // Metadata descriptor.
    crate_
        .graph
        .push(GraphVector::MetadataDescriptor(MetadataDescriptor {
            id: METADATA_FILE.to_string(),
            type_: DataType::Term("CreativeWork".to_string()),
            conforms_to: Id::Id("https://w3id.org/ro/crate/1.1".to_string()),
            about: Id::Id("./".to_string()),
            dynamic_entity: None,
        }));

    // Engine `SoftwareApplication`.
    crate_
        .graph
        .push(GraphVector::ContextualEntity(ContextualEntity {
            id: "#engine".to_string(),
            type_: DataType::Term("SoftwareApplication".to_string()),
            dynamic_entity: bag(vec![
                ("name", ev_str(&ctx.engine.name)),
                ("version", ev_str(&ctx.engine.version)),
                ("url", ev_str(&ctx.engine.repository)),
            ]),
        }));

    // Submitter agent — emitted only when a real submitter is known; never
    // invented.
    let agent_name = ctx
        .session
        .map(|s| s.created_by.as_str())
        .filter(|n| !n.is_empty());
    if let Some(name) = agent_name {
        crate_
            .graph
            .push(GraphVector::ContextualEntity(ContextualEntity {
                id: "#agent".to_string(),
                type_: DataType::Term("Person".to_string()),
                dynamic_entity: bag(vec![("name", ev_str(name))]),
            }));
    }

    // Workflow entity (the materialized WDL source; written by `materialize_sources`).
    let mut workflow_props = vec![
        ("name", ev_str(&wf_name)),
        ("description", ev_str(&wf_desc)),
        ("url", ev_str(&ctx.run.source)),
        ("programmingLanguage", ev_id("#wdl")),
    ];
    if !input_param_ids.is_empty() {
        workflow_props.push(("input", ev_ids(input_param_ids)));
    }
    if !output_param_ids.is_empty() {
        workflow_props.push(("output", ev_ids(output_param_ids)));
    }
    crate_.graph.push(GraphVector::DataEntity(DataEntity {
        id: WORKFLOW_ID.to_string(),
        type_: DataType::TermArray(vec![
            "File".to_string(),
            "SoftwareSourceCode".to_string(),
            "ComputationalWorkflow".to_string(),
        ]),
        dynamic_entity: bag(workflow_props),
    }));
    crate_
        .graph
        .push(GraphVector::ContextualEntity(ContextualEntity {
            id: "#wdl".to_string(),
            type_: DataType::Term("ComputerLanguage".to_string()),
            dynamic_entity: bag(vec![
                ("name", ev_str("WDL")),
                ("url", ev_str("https://openwdl.org")),
            ]),
        }));

    // Formal parameters from the selected callable's interface.
    for (name, input) in inputs_iface {
        let pid = format!("#param-in-{name}");
        crate_
            .graph
            .push(formal_parameter(&pid, &name, input.ty(), input.required()));
    }
    for (name, output) in outputs_iface {
        let pid = format!("#param-out-{name}");
        crate_.graph.push(formal_parameter(
            &pid,
            &name,
            output.ty(),
            !output.ty().is_optional(),
        ));
    }

    // Realized inputs/outputs.
    let mut input_ids = Vec::new();
    for (name, value) in ctx.inputs_iter() {
        let id = value_to_entities("input", name, value, crate_root, opts, &mut crate_.graph)?;
        if let Some(param_id) = input_params.get(name) {
            let mut property = HashMap::new();
            property.insert("exampleOfWork".to_string(), ev_id(param_id));
            crate_.add_dynamic_entity_property(&id, property);
        }
        input_ids.push(id);
    }
    let mut output_ids = Vec::new();
    for (name, value) in ctx.outputs.iter() {
        let id = value_to_entities("output", name, value, crate_root, opts, &mut crate_.graph)?;
        if let Some(param_id) = output_params.get(name) {
            let mut property = HashMap::new();
            property.insert("exampleOfWork".to_string(), ev_id(param_id));
            crate_.add_dynamic_entity_property(&id, property);
        }
        output_ids.push(id);
    }

    // Engine `OrganizeAction`.
    let mut organize_props = vec![
        ("name", ev_str("Workflow orchestration")),
        ("agent", ev_id("#engine")),
        ("instrument", ev_id("#engine")),
        ("object", ev_id("#run")),
    ];
    if let Some(s) = &started {
        organize_props.push(("startTime", ev_str(s)));
    }
    if let Some(e) = &ended {
        organize_props.push(("endTime", ev_str(e)));
    }
    crate_
        .graph
        .push(GraphVector::ContextualEntity(ContextualEntity {
            id: "#organize".to_string(),
            type_: DataType::Term("OrganizeAction".to_string()),
            dynamic_entity: bag(organize_props),
        }));

    // Workflow-level `CreateAction`.
    let mut run_props = vec![
        ("name", ev_str(&ctx.run.name)),
        ("instrument", ev_id(WORKFLOW_ID)),
        ("object", ev_ids(input_ids.clone())),
        ("result", ev_ids(output_ids.clone())),
        (
            "actionStatus",
            ev_id("http://schema.org/CompletedActionStatus"),
        ),
    ];
    if agent_name.is_some() {
        run_props.push(("agent", ev_id("#agent")));
    }
    if let Some(s) = &started {
        run_props.push(("startTime", ev_str(s)));
    }
    if let Some(e) = &ended {
        run_props.push(("endTime", ev_str(e)));
    }
    crate_
        .graph
        .push(GraphVector::ContextualEntity(ContextualEntity {
            id: "#run".to_string(),
            type_: DataType::Term("CreateAction".to_string()),
            dynamic_entity: bag(run_props),
        }));

    // Root dataset.
    let mut mentions = vec!["#run".to_string(), "#organize".to_string()];
    mentions.extend(input_ids);
    mentions.extend(output_ids);
    let mut root_props = vec![
        (
            "conformsTo",
            ev_ids(PROFILES.iter().map(|s| s.to_string()).collect()),
        ),
        ("mainEntity", ev_id(WORKFLOW_ID)),
        ("mentions", ev_ids(mentions)),
        ("hasPart", ev_id(WORKFLOW_ID)),
    ];
    if agent_name.is_some() {
        root_props.push(("author", ev_id("#agent")));
    }
    crate_
        .graph
        .push(GraphVector::RootDataEntity(RootDataEntity {
            id: "./".to_string(),
            type_: DataType::Term("Dataset".to_string()),
            name: ctx.run.name.clone(),
            description: wf_desc,
            date_published,
            // Required by the data model; Sprocket does not assert a license for
            // the run's data, so this honestly records that none was specified.
            license: License::Description("license not specified".to_string()),
            dynamic_entity: bag(root_props),
        }));

    Ok(crate_)
}

/// Appends materialized import `@id`s to the workflow entity's `hasPart`.
pub fn add_workflow_parts(crate_: &mut RoCrate, import_ids: &[String]) {
    if import_ids.is_empty() {
        return;
    }
    let mut parts = vec![WORKFLOW_ID.to_string()];
    parts.extend(import_ids.iter().cloned());
    let mut property = HashMap::new();
    property.insert("hasPart".to_string(), ev_ids(parts));
    crate_.add_dynamic_entity_property(WORKFLOW_ID, property);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_workflow_parts_links_imports() {
        let mut crate_ = RoCrate {
            context: RoCrateContext::ReferenceContext(ROCRATE_CONTEXT.to_string()),
            graph: vec![GraphVector::DataEntity(DataEntity {
                id: WORKFLOW_ID.to_string(),
                type_: DataType::Term("File".to_string()),
                dynamic_entity: None,
            })],
        };
        add_workflow_parts(&mut crate_, &["workflow/tasks.wdl".to_string()]);
        let json = serde_json::to_string(&crate_).unwrap();
        assert!(json.contains("hasPart"));
        assert!(json.contains("workflow/tasks.wdl"));
    }
}

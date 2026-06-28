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
use super::PROCESS_PROFILE;
use super::PROFILES;
use super::PROVENANCE_RUN_CRATE_PROFILE;
use super::ROCRATE_CONTEXT;
use super::RoCrateOptions;
use super::RunCrateContext;
use super::WFRUN_CONTEXT;
use super::WORKFLOW_ID;
use super::WORKFLOW_RO_CRATE_PROFILE;
use super::WORKFLOW_RUN_CRATE_PROFILE;
use super::formal::formal_parameter;
use super::value::sanitize_component;
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

/// Extracts `@id` references from a single or array id value.
fn id_value_ids(value: EntityValue) -> Vec<String> {
    match value {
        EntityValue::EntityId(Id::Id(id)) => vec![id],
        EntityValue::EntityId(Id::IdArray(ids)) => ids,
        _ => Vec::new(),
    }
}

/// Wraps a string literal as an `EntityValue`.
fn ev_str(s: &str) -> EntityValue {
    EntityValue::EntityString(s.to_string())
}

/// Builds the `dynamic_entity` map from `(key, value)` pairs.
fn bag(pairs: Vec<(&str, EntityValue)>) -> Option<HashMap<String, EntityValue>> {
    Some(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// Builds the `CreativeWork` contextual entity that a `conformsTo` profile URI
/// resolves to, so the reference is not dangling.
fn profile_entity(id: &str) -> GraphVector {
    let (name, version) = match id {
        PROCESS_PROFILE => ("Process Run Crate", "0.1"),
        WORKFLOW_RUN_CRATE_PROFILE => ("Workflow Run Crate", "0.1"),
        PROVENANCE_RUN_CRATE_PROFILE => ("Provenance Run Crate", "0.1"),
        WORKFLOW_RO_CRATE_PROFILE => ("Workflow RO-Crate", "1.0"),
        _ => ("RO-Crate profile", ""),
    };
    let mut props = vec![("name", ev_str(name))];
    if !version.is_empty() {
        props.push(("version", ev_str(version)));
    }
    GraphVector::ContextualEntity(ContextualEntity {
        id: id.to_string(),
        type_: DataType::Term("CreativeWork".to_string()),
        dynamic_entity: bag(props),
    })
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

    // Workflow targets emit a Workflow Run Crate with a `ComputationalWorkflow`
    // main entity. Direct task targets emit a Process Run Crate: a task is a
    // single process, not a workflow, so we type its WDL source as plain
    // `SoftwareSourceCode` and claim only the Process Run Crate profile rather
    // than fabricating a workflow that does not exist.
    let is_workflow = matches!(ctx.target, Target::Workflow(_));
    let wf_name = ctx.target.name().to_string();
    let (wf_desc, mut wf_types, mut profiles): (String, Vec<String>, Vec<&str>) = if is_workflow {
        (
            format!("WDL workflow `{wf_name}`"),
            vec![
                "File".to_string(),
                "SoftwareSourceCode".to_string(),
                "ComputationalWorkflow".to_string(),
            ],
            PROFILES.to_vec(),
        )
    } else {
        (
            format!("WDL task `{wf_name}`"),
            vec!["File".to_string(), "SoftwareSourceCode".to_string()],
            vec![PROCESS_PROFILE],
        )
    };
    if is_workflow {
        profiles.push(PROVENANCE_RUN_CRATE_PROFILE);
    }
    let mut task_step_ids = Vec::new();
    let mut task_control_ids = Vec::new();
    let mut task_tool_ids = Vec::new();
    let mut data_parts = Vec::new();
    if is_workflow && !ctx.tasks.is_empty() {
        for (index, task) in ctx.tasks.iter().enumerate() {
            let ids = super::tasks::task_step_entities(task, index + 1, &mut crate_.graph);
            let slug = sanitize_component(&task.name);
            if let Some(log) = ctx.task_logs.iter().find(|log| log.task_name == task.name) {
                let log_ids = super::tasks::task_log_entities(
                    &slug,
                    &ids.action,
                    log,
                    crate_root,
                    opts,
                    &mut crate_.graph,
                )?;
                data_parts.extend(log_ids);
            }
            task_step_ids.push(ids.step);
            task_control_ids.push(ids.control);
            task_tool_ids.push(ids.tool);
        }
    }
    if !task_step_ids.is_empty() {
        wf_types.push("HowTo".to_string());
    }
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
    // Use the canonical ISO 8601 / `xsd:dateTime` form (`...Z`, no fractional
    // seconds) that RO-Crate consumers and validators expect.
    let iso =
        |t: chrono::DateTime<chrono::Utc>| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let started = ctx.run.started_at.map(iso);
    let ended = ctx.run.completed_at.map(iso);
    // `date_published` is required on the root dataset; use a real run timestamp
    // (start when known, else the creation time) rather than inventing one.
    let date_published = iso(ctx.run.started_at.unwrap_or(ctx.run.created_at));

    // Metadata descriptor. Per Workflow RO-Crate, it conforms to RO-Crate 1.1 and
    // (for workflow targets) the Workflow RO-Crate profile.
    let descriptor_conforms = if is_workflow {
        Id::IdArray(vec![
            "https://w3id.org/ro/crate/1.1".to_string(),
            WORKFLOW_RO_CRATE_PROFILE.to_string(),
        ])
    } else {
        Id::Id("https://w3id.org/ro/crate/1.1".to_string())
    };
    crate_
        .graph
        .push(GraphVector::MetadataDescriptor(MetadataDescriptor {
            id: METADATA_FILE.to_string(),
            type_: DataType::Term("CreativeWork".to_string()),
            conforms_to: descriptor_conforms,
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

    // Main executable entity (the materialized WDL source, written by
    // `materialize_sources`).
    let mut workflow_props = vec![
        ("name", ev_str(&wf_name)),
        ("description", ev_str(&wf_desc)),
        ("programmingLanguage", ev_id("#wdl")),
    ];
    // Record the source location only when it is a remote URL; a local source
    // path would leak host filesystem layout.
    if ctx.run.source.contains("://") {
        workflow_props.push(("url", ev_str(&ctx.run.source)));
    }
    // `input`/`output` are `ComputationalWorkflow` properties; only attach them to
    // a workflow entity. For task targets the formal parameters are still emitted
    // and linked from the realized values via `exampleOfWork`.
    if is_workflow {
        if !input_param_ids.is_empty() {
            workflow_props.push(("input", ev_ids(input_param_ids)));
        }
        if !output_param_ids.is_empty() {
            workflow_props.push(("output", ev_ids(output_param_ids)));
        }
        if !task_step_ids.is_empty() {
            workflow_props.push(("step", ev_ids(task_step_ids)));
            workflow_props.push(("hasPart", ev_ids(task_tool_ids)));
        }
    }
    crate_.graph.push(GraphVector::DataEntity(DataEntity {
        id: WORKFLOW_ID.to_string(),
        type_: DataType::TermArray(wf_types),
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

    // Realized inputs/outputs. Crate-contained data entities (those whose `@id`
    // is a crate-relative path, not a `#`-prefixed `PropertyValue`) are collected
    // so the root dataset can reference them from `hasPart`.
    let mut input_ids = Vec::new();
    for (name, value) in ctx.inputs_iter() {
        let id = value_to_entities("input", name, value, crate_root, opts, &mut crate_.graph)?;
        if let Some(param_id) = input_params.get(name) {
            let mut property = HashMap::new();
            property.insert("exampleOfWork".to_string(), ev_id(param_id));
            crate_.add_dynamic_entity_property(&id, property);
        }
        if !id.starts_with('#') {
            data_parts.push(id.clone());
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
        if !id.starts_with('#') {
            data_parts.push(id.clone());
        }
        output_ids.push(id);
    }

    // Engine `OrganizeAction` — the workflow engine orchestrating the run. Only
    // meaningful for workflow targets; a single task is one process with no
    // orchestration. Its `result` is the run `CreateAction` (`object` is reserved
    // for the step/control actions added with step-level provenance later).
    if is_workflow {
        let mut organize_props = vec![
            ("name", ev_str("Workflow orchestration")),
            ("instrument", ev_id("#engine")),
            ("result", ev_id("#run")),
        ];
        // The agent is the submitter (when known); the engine is the instrument.
        if agent_name.is_some() {
            organize_props.push(("agent", ev_id("#agent")));
        }
        if !task_control_ids.is_empty() {
            organize_props.push(("object", ev_ids(task_control_ids)));
        }
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
    }

    // Workflow-level `CreateAction`.
    let run_description = format!("Execution of {wf_desc}");
    let mut run_props = vec![
        ("name", ev_str(&ctx.run.name)),
        ("description", ev_str(&run_description)),
        ("instrument", ev_id(WORKFLOW_ID)),
        (
            "actionStatus",
            ev_id("http://schema.org/CompletedActionStatus"),
        ),
    ];
    // Omit empty `object`/`result` rather than emitting empty arrays.
    if !input_ids.is_empty() {
        run_props.push(("object", ev_ids(input_ids.clone())));
    }
    if !output_ids.is_empty() {
        run_props.push(("result", ev_ids(output_ids.clone())));
    }
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

    // Define a `CreativeWork` entity for each conformance profile so the
    // `conformsTo` references resolve.
    for profile in &profiles {
        crate_.graph.push(profile_entity(profile));
    }

    // Root dataset. `mentions` references the action entities that describe the
    // run (data entities are referenced from `hasPart`, not `mentions`).
    let mut mentions = vec!["#run".to_string()];
    if is_workflow {
        mentions.push("#organize".to_string());
    }
    // `hasPart` lists the crate-contained data entities: the main WDL source and
    // every localized/in-place input/output file or directory.
    let mut has_part = vec![WORKFLOW_ID.to_string()];
    has_part.extend(data_parts);
    let mut root_props = vec![
        (
            "conformsTo",
            ev_ids(profiles.iter().map(|s| s.to_string()).collect()),
        ),
        ("mainEntity", ev_id(WORKFLOW_ID)),
        ("mentions", ev_ids(mentions)),
        ("hasPart", ev_ids(has_part)),
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

/// Links materialized import `@id`s to the root dataset's `hasPart`.
pub fn add_import_parts(crate_: &mut RoCrate, import_ids: &[String]) {
    if import_ids.is_empty() {
        return;
    }
    for entity in &mut crate_.graph {
        let GraphVector::RootDataEntity(root) = entity else {
            continue;
        };
        if root.id != "./" {
            continue;
        }

        let dynamic = root.dynamic_entity.get_or_insert_with(HashMap::new);
        let mut ids = dynamic
            .remove("hasPart")
            .map(id_value_ids)
            .unwrap_or_default();
        for id in import_ids {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
        dynamic.insert("hasPart".to_string(), ev_ids(ids));
        return;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_import_parts_links_imports_to_root() {
        let mut crate_ = RoCrate {
            context: RoCrateContext::ReferenceContext(ROCRATE_CONTEXT.to_string()),
            graph: vec![GraphVector::RootDataEntity(RootDataEntity {
                id: "./".to_string(),
                type_: DataType::Term("Dataset".to_string()),
                name: "test run".to_string(),
                description: "test crate".to_string(),
                date_published: "2026-01-01T00:00:00Z".to_string(),
                license: License::Description("license not specified".to_string()),
                dynamic_entity: bag(vec![("hasPart", ev_ids(vec!["#tool-task".to_string()]))]),
            })],
        };
        add_import_parts(&mut crate_, &["workflow/tasks.wdl".to_string()]);
        let json = serde_json::to_string(&crate_).unwrap();
        assert!(json.contains("hasPart"));
        assert!(json.contains("#tool-task"));
        assert!(json.contains("workflow/tasks.wdl"));
    }
}

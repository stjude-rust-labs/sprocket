//! Implementation of the `input` command.

use std::collections::HashSet;

use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use clap::Parser;
use serde_json::Map;
use serde_json::Value;
use serde_yaml_ng;
use wdl::analysis::Document;
use wdl::analysis::types::CallKind;
use wdl::ast::AstNode;
use wdl::ast::AstToken;
use wdl::ast::v1::Decl;
use wdl::ast::v1::Expr;
use wdl::ast::v1::InputSection;
use wdl::ast::v1::LiteralExpr;
use wdl::ast::v1::TaskDefinition;
use wdl::ast::v1::Type;
use wdl::cli::Analysis;
use wdl::cli::analysis::Source;

/// Arguments for the `input` subcommand.
#[derive(Parser, Debug)]
pub struct Args {
    /// A source WDL file or URL.
    #[arg(value_name = "PATH or URL")]
    pub source: Source,

    /// The name of the task or workflow for which to generate inputs.
    #[clap(short, long, value_name = "NAME")]
    pub name: Option<String>,

    /// Show inputs with non-literal default values.
    #[arg(long)]
    pub show_expressions: bool,

    /// Hide inputs with default values.
    #[arg(long, conflicts_with = "show_expressions")]
    pub hide_defaults: bool,

    /// Generate inputs for all tasks called in the workflow.  
    #[arg(long)]
    pub include_nested_inputs: bool,

    /// Output the template as a YAML file.
    #[arg(long)]
    pub yaml: bool,
}

/// An input key.
#[derive(Clone, Debug)]
pub struct Key(Vec<String>);

impl Key {
    /// Creates a new key with a preinitialized value.
    pub fn new(value: String) -> Self {
        Self(vec![value])
    }

    /// Creates a new, empty key.
    pub fn empty() -> Self {
        Self(vec![])
    }

    /// Pushes a value into the key.
    pub fn push(mut self, value: impl Into<String>) -> Self {
        self.0.push(value.into());
        self
    }

    /// Joins the key using `.` as the delimeter.
    pub fn join(self) -> Option<String> {
        if self.0.is_empty() {
            return None;
        }

        Some(self.0.join("."))
    }
}

/// An input processor.
#[derive(Debug)]
pub struct InputProcessor {
    /// The results of the input processing.
    results: Map<String, Value>,

    /// Whether or not to include nested inputs.
    include_nested_inputs: bool,

    /// Whether or not to show expressions.
    show_expressions: bool,

    /// Whether or not to include defaults.
    hide_defaults: bool,
}

impl InputProcessor {
    /// Creates a new input processor.
    pub fn new(include_nested_inputs: bool, show_expressions: bool, hide_defaults: bool) -> Self {
        Self {
            results: Default::default(),
            include_nested_inputs,
            show_expressions,
            hide_defaults,
        }
    }

    /// Consumes `self` and returns the inner results.
    pub fn into_inner(self) -> Map<String, Value> {
        self.results
    }

    /// Processes an expression.
    fn expression(
        &self,
        ty: Type,
        namespace: Key,
        name: &str,
        expr: &Expr,
    ) -> Option<(Key, Value)> {
        match expr {
            Expr::Literal(l) if !self.hide_defaults => match l {
                LiteralExpr::Boolean(v) => {
                    return Some((namespace.push(name), Value::Bool(v.value())));
                }
                LiteralExpr::String(v) => {
                    if let Some(text) = v.text() {
                        let mut buffer = String::new();
                        text.unescape_to(&mut buffer);
                        return Some((namespace.push(name), Value::String(buffer)));
                    } else {
                        return Some((namespace.push(name), Value::String(Default::default())));
                    }
                }
                LiteralExpr::Integer(v) => {
                    return Some((namespace.push(name), Value::from(v.value().unwrap_or(0))));
                }
                LiteralExpr::Float(v) => {
                    return Some((namespace.push(name), Value::from(v.value().unwrap_or(0.0))));
                }
                LiteralExpr::Struct(v) => {
                    let map = v
                        .items()
                        .filter_map(|item| {
                            let (name, value) = item.name_value();
                            self.expression(ty.clone(), Key::empty(), name.text(), &value)
                        })
                        .map(|(k, v)| (k.join().expect("key to join"), v))
                        .collect::<Map<_, _>>();

                    return Some((namespace.push(name), Value::Object(map)));
                }
                LiteralExpr::Array(a) => {
                    let mut values = vec![];

                    for item in a.elements() {
                        if let Some((key, value)) =
                            self.expression(ty.clone(), Key::empty(), name, &item)
                        {
                            values.push((key.join().expect("key to join"), value));
                        }
                    }

                    return Some((
                        namespace.push(name),
                        Value::Array(values.into_iter().map(|(_, v)| v).collect()),
                    ));
                }
                LiteralExpr::Map(m) => {
                    let mut map = Map::new();

                    for item in m.items() {
                        let (k, v) = item.key_value();
                        let key_name: String = match k {
                            Expr::Literal(ref l) => match l {
                                LiteralExpr::String(k) => {
                                    if let Some(text) = k.text() {
                                        let mut buffer = String::new();
                                        text.unescape_to(&mut buffer);
                                        buffer
                                    } else {
                                        String::new()
                                    }
                                }
                                _ => k.text().to_string(),
                            },
                            _ => k.text().to_string(),
                        };

                        if let Some((_key, value)) =
                            self.expression(ty.clone(), Key::empty(), name, &v)
                        {
                            map.insert(key_name, value);
                        }
                    }

                    return Some((namespace.push(name), Value::Object(map)));
                }
                LiteralExpr::Pair(p) => {
                    let mut map = Map::new();
                    let (left, right) = p.exprs();

                    if let Some((_key, value)) =
                    self.expression(ty.clone(), Key::empty(), name, &left)
                    {
                        map.insert("left".to_string(), value);
                    }

                    if let Some((_key, value)) =
                    self.expression(ty.clone(), Key::empty(), name, &right)
                    {
                        map.insert("right".to_string(), value);
                    }

                    return Some((namespace.push(name), Value::Object(map)));
                }
                LiteralExpr::Object(v) => {
                    let map = v
                        .items()
                        .filter_map(|item| {
                            let (name, value) = item.name_value();
                            self.expression(ty.clone(), Key::empty(), name.text(), &value)
                        })
                        .map(|(k, v)| (k.join().expect("key to join"), v))
                        .collect::<Map<_, _>>();

                    return Some((namespace.push(name), Value::Object(map)));
                }
                _ => {
                    let value = expr.text().to_string();

                    return Some((namespace.push(name), Value::String(value)));
                }
            },
            Expr::Negation(v) if !self.hide_defaults => {
                if let Expr::Literal(literal) = v.operand() {
                    if let LiteralExpr::Boolean(b) = literal {
                        return Some((namespace.push(name), Value::Bool(!b.value())));
                    } else if let LiteralExpr::Integer(n) = literal {
                        return Some((
                            namespace.push(name),
                            Value::from(n.value().map(|v| -v).unwrap_or_default()),
                        ));
                    } else if let LiteralExpr::Float(n) = literal {
                        return Some((
                            namespace.push(name),
                            Value::from(n.value().map(|v| -v).unwrap_or_default()),
                        ));
                    }
                }
            }
            _ => {}
        }

        if self.show_expressions {
            let mut value = ty.to_string();
            value.push_str(" (default = ");
            value.push_str(&expr.text().to_string());
            value.push(')');
            Some((namespace.push(name), Value::String(value)))
        } else {
            None
        }
    }

    /// Processes an input section.
    fn input_section(&mut self, namespace: Key, input_section: InputSection) {
        for decl in input_section.declarations() {
            match decl {
                Decl::Bound(decl) => {
                    let name = decl.name();
                    let ty = decl.ty();
                    let expr = decl.expr();

                    if ty.is_optional() {
                        if !self.hide_defaults {
                            let mut value = ty.to_string();

                            if self.show_expressions {
                                value.push_str(" (default = ");
                                value.push_str(&expr.text().to_string());
                                value.push(')');
                            }

                            self.results.insert(
                                namespace
                                    .clone()
                                    .push(name.text())
                                    .join()
                                    .expect("key to join"),
                                Value::String(value),
                            );
                        }
                    } else if let Some((key, value)) =
                        self.expression(ty, namespace.clone(), name.text(), &expr)
                    {
                        self.results.insert(key.join().expect("key to join"), value);
                    }
                }
                Decl::Unbound(decl) => {
                    let name = decl.name();
                    let ty = decl.ty();

                    if ty.is_optional() {
                        if !self.hide_defaults {
                            self.results.insert(
                                namespace
                                    .clone()
                                    .push(name.text())
                                    .join()
                                    .expect("key to join"),
                                Value::Null,
                            );
                        }
                    } else {
                        self.results.insert(
                            namespace
                                .clone()
                                .push(name.text())
                                .join()
                                .expect("key to join"),
                            Value::String(ty.to_string()),
                        );
                    }
                }
            }
        }
    }

    /// Processes a task.
    fn task(&mut self, namespace: Key, task: &TaskDefinition, specified: &HashSet<String>) {
        if let Some(inputs) = task.input() {
            self.input_section(namespace.clone(), inputs);

            // Any inputs specified by the call itself cannot be overridden.
            specified.iter().for_each(|s| {
                let key = namespace.clone().push(s).join().expect("key to join");
                self.results.remove(&key);
            });
        }
    }

    /// Processes a workflow.
    fn workflow(
        &mut self,
        namespace: Key,
        document: &Document,
        analysis_wf: &wdl::analysis::document::Workflow,
        ast_wf: &wdl::ast::v1::WorkflowDefinition,
    ) -> Result<()> {
        if let Some(inputs) = ast_wf.input() {
            self.input_section(namespace.clone(), inputs);
        }

        if self.include_nested_inputs && analysis_wf.allows_nested_inputs() {
            for (call_name, call) in analysis_wf.calls() {
                let namespace = namespace.clone().push(call_name);

                match call.kind() {
                    CallKind::Task => {
                        let name = call.name();
                        let specified = call.specified();

                        fn get_task_def(document: &Document, name: &str) -> Result<TaskDefinition> {
                            let ast = document.root().ast().into_v1().ok_or(anyhow!(
                                "non-v1 WDL document `{}` cannot be processed with this subcommand",
                                document.uri()
                            ))?;

                            Ok(ast
                                .tasks()
                                .find(|task| task.name().text() == name)
                                .expect("referenced task to be present"))
                        }

                        if let Some(ns) = call.namespace() {
                            // The task was imported from another namespace.
                            let document = document
                                .namespace(ns)
                                .expect("referenced namespace should be present")
                                .document();

                            let task = get_task_def(document, name)?;
                            self.task(namespace, &task, specified);
                        } else {
                            // The task is in the current document.
                            let task = get_task_def(document, name)?;
                            self.task(namespace, &task, specified);
                        }
                    }
                    CallKind::Workflow => {
                        // An imported subworkflow.
                        let name = call.name();
                        let specified = call.specified();

                        let document = document
                            .namespace(
                                call.namespace()
                                    .expect("subworkflows will always have a namespace"),
                            )
                            .expect("referenced namespace should be present")
                            .document();

                        let ast = document.root().ast().into_v1().ok_or(anyhow!(
                            "non-v1 WDL document `{}` cannot be processed with this subcommand",
                            document.uri()
                        ))?;

                        let workflow = ast
                            .workflows()
                            .find(|workflow| workflow.name().text() == name)
                            .expect("referenced workflow to be present");

                        self.workflow(
                            namespace.clone(),
                            document,
                            document.workflow().expect("workflow to be present"),
                            &workflow,
                        )?;

                        // Any inputs specified by the workflow itself cannot be overridden.
                        specified.iter().for_each(|s| {
                            let key = namespace.clone().push(s).join().expect("key to join");
                            self.results.remove(&key);
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

/// Displays the input schema for a WDL document.
pub async fn inputs(args: Args) -> Result<()> {
    if let Source::Directory(_) = args.source {
        bail!("directory sources are not supported for the `inputs` command");
    }
    let results = match Analysis::default()
        .add_source(args.source.clone())
        .run()
        .await
    {
        Ok(results) => results,
        Err(errors) => {
            // SAFETY: this is a non-empty, so it must always have a first
            // element.
            bail!(errors.into_iter().next().unwrap())
        }
    };

    let document = results
        .filter(&[&args.source])
        .next()
        .expect("the root source should always be included in the results")
        .document();

    let mut inputs = InputProcessor::new(
        args.include_nested_inputs,
        args.show_expressions,
        args.hide_defaults,
    );

    let ast = document.root().ast().into_v1().ok_or(anyhow!(
        "non-v1 WDL document `{}` cannot be processed with this subcommand",
        document.uri()
    ))?;

    if let Some(name) = args.name {
        let namespace = Key::new(name.to_owned());

        match (document.task_by_name(&name), document.workflow()) {
            (Some(_), _) => {
                // Task with name found.
                let task = ast
                    .tasks()
                    .find(|task| task.name().text() == name)
                    // SAFETY: we just checked that a task with this name should
                    // be found, so this should always unwrap.
                    .unwrap();

                inputs.task(namespace, &task, &Default::default());
            }
            (None, Some(analysis_wf)) => {
                if analysis_wf.name() != name {
                    bail!("no task or workflow with name `{name}` was found")
                }

                if !analysis_wf.allows_nested_inputs() && args.include_nested_inputs {
                    bail!("workflow `{name}` does not allow nested inputs");
                }

                let ast_wf = ast
                    .workflows()
                    .find(|workflow| workflow.name().text() == name)
                    // SAFETY: we just checked that a workflow with this name should
                    // be found, so this should always unwrap.
                    .unwrap();

                inputs.workflow(namespace, document, analysis_wf, &ast_wf)?;
            }
            (None, None) => bail!("no task or workflow with name `{name}` was found"),
        }
    } else if let Some(workflow) = document.workflow() {
        let name = workflow.name().to_owned();

        if !workflow.allows_nested_inputs() && args.include_nested_inputs {
            bail!("workflow `{name}` does not allow nested inputs");
        }

        let namespace = Key::new(name.clone());

        let ast_wf = ast
            .workflows()
            .find(|workflow| workflow.name().text() == name)
            // SAFETY: we just checked that a workflow with this name should
            // be found, so this should always unwrap.
            .unwrap();

        inputs.workflow(namespace, document, workflow, &ast_wf)?;
    } else {
        bail!(
            "no workflow was found; try specifying a task or workflow name with the `--name` \
             argument"
        )
    }

    let inputs = inputs.into_inner();

    if args.yaml {
        let yaml = serde_yaml_ng::to_string(&inputs)?;
        println!("{}", yaml);
    } else {
        let json = serde_json::to_string_pretty(&inputs)?;
        println!("{}", json);
    }

    Ok(())
}

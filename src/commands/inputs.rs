//! Implementation of the `inputs` command.

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
use wdl::ast::v1::StringPart;
use wdl::ast::v1::TaskDefinition;
use wdl::cli::Analysis;
use wdl::cli::analysis::Source;

/// Arguments for the `inputs` subcommand.
#[derive(Parser, Debug)]
pub struct Args {
    /// A source WDL document or URL.
    #[arg(value_name = "SOURCE")]
    pub source: Source,

    /// The name of the task or workflow for which to generate inputs.
    #[clap(short, long, value_name = "NAME")]
    pub name: Option<String>,

    /// Show inputs with non-literal default values.
    #[arg(long)]
    pub show_non_literals: bool,

    /// Hide inputs with default values.
    #[arg(long, conflicts_with = "show_non_literals")]
    pub hide_defaults: bool,

    /// Generate inputs for all tasks called in the workflow.  
    #[arg(long)]
    pub nested_inputs: bool,

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

    /// Joins the key using `.` as the delimiter.
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
    fn expression(&self, expr: &Expr) -> Option<Value> {
        let literal_to_value = |literal: &LiteralExpr| -> Option<Value> {
            match literal {
                LiteralExpr::Boolean(b) => Some(Value::Bool(b.value())),
                LiteralExpr::Float(f) => match f.value() {
                    Some(f) => Some(Value::from(f)),
                    None if self.show_expressions => {
                        Some(Value::from("Float <DEFAULT IS OUT OF RANGE>"))
                    }
                    None => None,
                },
                LiteralExpr::Integer(i) => match i.value() {
                    Some(i) => Some(Value::Number(i.into())),
                    None if self.show_expressions => {
                        Some(Value::from("Int <DEFAULT IS OUT OF RANGE>"))
                    }
                    None => None,
                },
                LiteralExpr::None(_) => Some(Value::Null),
                LiteralExpr::String(s) => match s.text() {
                    Some(text) => Some(Value::from(text.text())),
                    None if self.show_expressions => {
                        let merged_parts = s
                            .parts()
                            .map(|p| match p {
                                StringPart::Placeholder(placeholder) => {
                                    placeholder.text().to_string()
                                }
                                StringPart::Text(text) => {
                                    let mut buff = String::new();
                                    text.unescape_to(&mut buff);
                                    buff
                                }
                            })
                            .collect::<String>();
                        Some(Value::String(format!(
                            "String <NON-LITERAL: `{merged_parts}`>"
                        )))
                    }
                    None => None,
                },
                LiteralExpr::Array(a) => {
                    let mut values = vec![];
                    for elem in a.elements() {
                        if let Some(val) = self.expression(&elem) {
                            values.push(val);
                        } else if self.show_expressions {
                            values.push(Value::String(format!(
                                "<NON-LITERAL: `{expr}`>",
                                expr = elem.text()
                            )))
                        } else {
                            values.push(Value::from("<OMITTED>"))
                        }
                    }
                    Some(Value::from(values))
                }
                LiteralExpr::Pair(p) => {
                    let (left, right) = p.exprs();

                    let mut map = Map::new();
                    if let Some(left) = self.expression(&left) {
                        map.insert("left".to_string(), left);
                    } else if self.show_expressions {
                        map.insert(
                            "left".to_string(),
                            Value::String(format!("<NON-LITERAL: `{expr}`>", expr = left.text())),
                        );
                    } else {
                        map.insert("left".to_string(), Value::from("<OMITTED>"));
                    }
                    if let Some(right) = self.expression(&right) {
                        map.insert("right".to_string(), right);
                    } else if self.show_expressions {
                        map.insert(
                            "right".to_string(),
                            Value::String(format!("<NON-LITERAL: `{expr}`>", expr = right.text())),
                        );
                    } else {
                        map.insert("right".to_string(), Value::from("<OMITTED>"));
                    }
                    Some(Value::Object(map))
                }
                LiteralExpr::Map(m) => {
                    let mut map = Map::new();
                    let mut bad_key_counter = 0_usize;
                    for item in m.items() {
                        let (key, val) = item.key_value();
                        let key = if let Some(literal) = key.as_literal()
                            && let Some(string) = literal.as_string()
                            && let Some(text) = string.text()
                        {
                            text.text().to_string()
                        } else {
                            bad_key_counter += 1;
                            format!("<OMITTED_{bad_key_counter}>")
                        };
                        if let Some(val) = self.expression(&val) {
                            map.insert(key, val);
                        } else {
                            map.insert(key, Value::from("<OMITTED>"));
                        }
                    }
                    Some(Value::Object(map))
                }
                LiteralExpr::Struct(s) => {
                    let mut map = Map::new();
                    for item in s.items() {
                        let (key, val) = item.name_value();
                        if let Some(val) = self.expression(&val) {
                            map.insert(key.text().to_string(), val);
                        } else if self.show_expressions {
                            map.insert(
                                key.text().to_string(),
                                Value::String(format!(
                                    "<NON-LITERAL: `{expr}`>",
                                    expr = val.text()
                                )),
                            );
                        } else {
                            map.insert(key.text().to_string(), Value::from("<OMITTED>"));
                        }
                    }
                    Some(Value::Object(map))
                }
                LiteralExpr::Object(o) => {
                    let mut map = Map::new();
                    for item in o.items() {
                        let (key, val) = item.name_value();
                        if let Some(val) = self.expression(&val) {
                            map.insert(key.text().to_string(), val);
                        } else if self.show_expressions {
                            map.insert(
                                key.text().to_string(),
                                Value::String(format!(
                                    "<NON-LITERAL: `{expr}`>",
                                    expr = val.text()
                                )),
                            );
                        } else {
                            map.insert(key.text().to_string(), Value::from("<OMITTED>"));
                        }
                    }
                    Some(Value::Object(map))
                }
                _ => unreachable!("unexpected literal expression"),
            }
        };

        if let Some(literal) = expr.as_literal() {
            return literal_to_value(literal);
        };

        // attempt to recover negation expressions for numbers
        if let Some(negation) = expr.as_negation() {
            let positive_val = self.expression(&negation.operand())?;
            if let Some(num) = positive_val.as_number()
                && let Some(i) = num.as_i64()
            {
                return Some(Value::from(-i));
            }
            if let Some(num) = positive_val.as_number()
                && let Some(f) = num.as_f64()
            {
                return Some(Value::from(-f));
            }
        }
        None
    }

    /// Processes an input section.
    fn input_section(&mut self, namespace: Key, input_section: InputSection) {
        for decl in input_section.declarations() {
            match decl {
                Decl::Bound(decl) if !self.hide_defaults => {
                    let name = decl.name();
                    let expr = decl.expr();

                    if let Some(value) = self.expression(&expr) {
                        self.results
                            .insert(namespace.clone().push(name.text()).join().unwrap(), value);
                    } else if self.show_expressions {
                        self.results.insert(
                            namespace.clone().push(name.text()).join().unwrap(),
                            Value::from(format!(
                                "{ty} <NON-LITERAL: `{expr}`>",
                                ty = decl.ty(),
                                expr = expr.text()
                            )),
                        );
                    }
                }
                Decl::Unbound(decl) => {
                    let name = decl.name();
                    let ty = decl.ty();

                    if !ty.is_optional() {
                        // required input
                        self.results.insert(
                            namespace
                                .clone()
                                .push(name.text())
                                .join()
                                .expect("key to join"),
                            Value::String(format!("{ty} <REQUIRED>")),
                        );
                    } else if !self.hide_defaults {
                        self.results.insert(
                            namespace
                                .clone()
                                .push(name.text())
                                .join()
                                .expect("key to join"),
                            Value::Null,
                        );
                    }
                }
                _ => {
                    // default input we shouldn't insert
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

    let mut processor = InputProcessor::new(
        args.nested_inputs,
        args.show_non_literals,
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

                processor.task(namespace, &task, &Default::default());
            }
            (None, Some(analysis_wf)) => {
                if analysis_wf.name() != name {
                    bail!(
                        "no task or workflow with name `{name}` was found in document `{path}`",
                        path = document.path()
                    );
                }

                if !analysis_wf.allows_nested_inputs() && args.nested_inputs {
                    bail!("workflow `{name}` does not allow nested inputs");
                }

                let ast_wf = ast
                    .workflows()
                    .find(|workflow| workflow.name().text() == name)
                    // SAFETY: we just checked that a workflow with this name should
                    // be found, so this should always unwrap.
                    .unwrap();

                processor.workflow(namespace, document, analysis_wf, &ast_wf)?;
            }
            (None, None) => bail!(
                "no task or workflow with name `{name}` was found in document `{path}`",
                path = document.path()
            ),
        }
    } else if let Some(analysis_wf) = document.workflow() {
        let name = analysis_wf.name().to_owned();

        if !analysis_wf.allows_nested_inputs() && args.nested_inputs {
            bail!("workflow `{name}` does not allow nested inputs");
        }

        let namespace = Key::new(name.clone());

        let ast_wf = ast
            .workflows()
            .find(|workflow| workflow.name().text() == name)
            // SAFETY: we just checked that a workflow with this name should
            // be found, so this should always unwrap.
            .unwrap();

        processor.workflow(namespace, document, analysis_wf, &ast_wf)?;
    } else {
        let mut tasks = document.tasks();
        let first = tasks.next();
        if tasks.next().is_some() {
            bail!(
                "document `{path}` contains more than one task: use the `--name` option to refer \
                 to a specific task by name",
                path = document.path()
            )
        } else if let Some(task) = first {
            let namespace = Key::new(task.name().to_string());

            let task = ast
                .tasks()
                .find(|t| t.name().text() == task.name())
                // SAFETY: the task should be present, so this should always unwrap.
                .unwrap();

            processor.task(namespace, &task, &Default::default());
        } else {
            bail!(
                "document `{path}` contains no workflow or task",
                path = document.path()
            );
        }
    }

    let inputs = processor.into_inner();

    if args.yaml {
        let yaml = serde_yaml_ng::to_string(&inputs)?;
        println!("{yaml}");
    } else {
        let json = serde_json::to_string_pretty(&inputs)?;
        println!("{json}");
    }

    Ok(())
}

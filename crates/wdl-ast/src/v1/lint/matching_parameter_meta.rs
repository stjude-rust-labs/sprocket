//! Inputs within a task/workflow _must_ have a `parameter_meta` entry.

use std::collections::HashSet;
use std::collections::VecDeque;
use std::ops::Sub as _;

use nonempty::NonEmpty;
use wdl_core::concern::code;
use wdl_core::concern::lint;
use wdl_core::concern::lint::Group;
use wdl_core::concern::lint::Rule;
use wdl_core::concern::Code;
use wdl_core::fs::location::Located;
use wdl_core::fs::Location;
use wdl_core::Version;

use crate::v1;
use crate::v1::document::Task;
use crate::v1::document::Workflow;

/// The context within which a `matching_parameter_meta` error occurs.
enum Context<'a> {
    /// A missing `parameter_meta` entry for a task.
    Task(&'a Task),

    /// A missing `parameter_meta` entry for a workflow.
    Workflow(&'a Workflow),
}

impl<'a> std::fmt::Display for Context<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Context::Task(_) => write!(f, "task"),
            Context::Workflow(_) => write!(f, "workflow"),
        }
    }
}

/// Every input parameter within a task/workflow _must_ have a matching entry in
/// a `parameter_meta` block. The key must exactly match the name of the input.
#[derive(Debug)]
pub struct MatchingParameterMeta;

impl<'a> MatchingParameterMeta {
    /// Ensures that each input has a corresponding `parameter_meta` element.
    fn missing_parameter_meta<'b>(
        &self,
        parameter: &str,
        context: &Context<'b>,
        location: &Location,
    ) -> lint::Warning
    where
        Self: Rule<&'a v1::Document>,
    {
        // SAFETY: this error is written so that it will always unwrap.
        lint::warning::Builder::default()
            .code(self.code())
            .level(lint::Level::Medium)
            .group(lint::Group::Completeness)
            .push_location(location.clone())
            .subject(format!(
                "missing parameter meta within {}: {}",
                context, parameter
            ))
            .body(format!(
                "Each input parameter within a {} should have an associated \
                `parameter_meta` entry with a detailed description of the \
                input.",
                context
            ))
            .fix(
                "Add a key to a `parameter_meta` block matching the parameter's exact \
                name with a detailed description of the input.",
            )
            .try_build()
            .unwrap()
    }

    /// Ensures that each input has a corresponding `parameter_meta` element.
    fn extraneous_parameter_meta<'b>(
        &self,
        parameter: &str,
        context: &Context<'b>,
        location: &Location,
    ) -> lint::Warning
    where
        Self: Rule<&'a v1::Document>,
    {
        // SAFETY: this error is written so that it will always unwrap.
        lint::warning::Builder::default()
            .code(self.code())
            .level(lint::Level::Medium)
            .group(lint::Group::Completeness)
            .push_location(location.clone())
            .subject(format!(
                "extraneous parameter meta within {}: {}",
                context, parameter
            ))
            .body(
                "A parameter meta entry with no corresponding input \
                parameter was detected",
            )
            .fix("Remove the parameter meta entry")
            .try_build()
            .unwrap()
    }
}

impl<'a> Rule<&'a v1::Document> for MatchingParameterMeta {
    fn code(&self) -> Code {
        // SAFETY: this manually crafted to unwrap successfully every time.
        Code::try_new(code::Kind::Warning, Version::V1, 3).unwrap()
    }

    fn group(&self) -> lint::Group {
        Group::Completeness
    }

    fn check(&self, tree: &'a v1::Document) -> lint::Result {
        let mut warnings = VecDeque::new();

        for task in tree.tasks() {
            let context = Context::Task(task);

            let meta_keys = get_parameter_meta_keys(&context);
            let input_parameters = get_input_parameter_names(&context);

            warnings.extend(report_errors(&context, input_parameters, meta_keys));
        }

        if let Some(workflow) = tree.workflow() {
            let context = Context::Workflow(workflow);

            let meta_keys = get_parameter_meta_keys(&context);
            let input_parameters = get_input_parameter_names(&context);

            warnings.extend(report_errors(&context, input_parameters, meta_keys));
        }

        match warnings.pop_front() {
            Some(front) => {
                let mut results = NonEmpty::new(front);
                results.extend(warnings);
                Ok(Some(results))
            }
            None => Ok(None),
        }
    }
}

/// Gets the defined `parameter_meta` keys for this context.
fn get_parameter_meta_keys(context: &Context<'_>) -> HashSet<Located<String>> {
    match context {
        Context::Task(task) => task
            .parameter_metadata()
            .cloned()
            .into_iter()
            .flat_map(|meta| meta.into_inner().into_iter())
            .map(|(identifier, _)| identifier.map(|identifier| identifier.to_string()))
            .collect::<HashSet<_>>(),
        Context::Workflow(workflow) => workflow
            .parameter_metadata()
            .cloned()
            .into_iter()
            .flat_map(|meta| meta.into_inner().into_iter())
            .map(|(identifier, _)| identifier.map(|identifier| identifier.to_string()))
            .collect::<HashSet<_>>(),
    }
}

/// Gets the defined input parameter names for this context.
fn get_input_parameter_names(context: &Context<'_>) -> HashSet<Located<String>> {
    match context {
        Context::Task(task) => task
            .input()
            .cloned()
            .into_iter()
            .flat_map(|input| input.declarations().cloned())
            .flatten()
            .map(|located| located.map(|declaration| declaration.name().to_string()))
            .collect::<HashSet<_>>(),
        Context::Workflow(workflow) => workflow
            .input()
            .cloned()
            .into_iter()
            .flat_map(|input| input.declarations().cloned())
            .flatten()
            .map(|located| located.map(|declaration| declaration.name().to_string()))
            .collect::<HashSet<_>>(),
    }
}

/// Reports errors within a particular context for the given input parameters
/// and defined parameter meta keys.
fn report_errors(
    context: &Context<'_>,
    input_parameters: HashSet<Located<String>>,
    meta_keys: HashSet<Located<String>>,
) -> Vec<lint::Warning> {
    let mut results = Vec::new();

    // Report existing parameters that have no matching parameter meta entry.
    results.extend(
        input_parameters
            .sub(&meta_keys)
            .into_iter()
            .map(|parameter| {
                MatchingParameterMeta.missing_parameter_meta(
                    &parameter,
                    context,
                    parameter.location(),
                )
            }),
    );

    // Report existing parameters that have no matching parameter meta entry.
    results.extend(
        meta_keys
            .sub(&input_parameters)
            .into_iter()
            .map(|parameter| {
                MatchingParameterMeta.extraneous_parameter_meta(
                    &parameter,
                    context,
                    parameter.location(),
                )
            }),
    );

    results
}

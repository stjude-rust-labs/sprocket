//! V1 AST representation for task definitions.

use std::fmt;

use rowan::NodeOrToken;

use super::BoundDecl;
use super::Decl;
use super::Expr;
use super::LiteralBoolean;
use super::LiteralFloat;
use super::LiteralInteger;
use super::LiteralString;
use super::OpenHeredoc;
use super::Placeholder;
use super::StructDefinition;
use super::WorkflowDefinition;
use crate::AstNode;
use crate::AstToken;
use crate::Ident;
use crate::SyntaxKind;
use crate::SyntaxNode;
use crate::SyntaxToken;
use crate::TreeNode;
use crate::TreeToken;
use crate::v1::display::write_input_section;
use crate::v1::display::write_output_section;

pub mod common;
pub mod requirements;
pub mod runtime;

/// The set of all valid task fields and their descriptions for the implicit
/// `task` variable.
pub const TASK_FIELDS: &[(&str, &str)] = &[
    (TASK_FIELD_NAME, "The task name."),
    (
        TASK_FIELD_ID,
        "A String with the unique ID of the task. The execution engine may choose the format for \
         this ID, but it is suggested to include at least the following information:\nThe task \
         name\nThe task alias, if it differs from the task name\nThe index of the task instance, \
         if it is within a scatter statement",
    ),
    (
        TASK_FIELD_CONTAINER,
        "The URI String of the container in which the task is executing, or None if the task is \
         being executed in the host environment.",
    ),
    (
        TASK_FIELD_CPU,
        "The allocated number of cpus as a Float. Must be greater than 0.",
    ),
    (
        TASK_FIELD_MEMORY,
        "The allocated memory in bytes as an Int. Must be greater than 0.",
    ),
    (
        TASK_FIELD_GPU,
        "An Array[String] with one specification per allocated GPU. The specification is \
         execution engine-specific. If no GPUs were allocated, then the value must be an empty \
         array.",
    ),
    (
        TASK_FIELD_FPGA,
        "An Array[String] with one specification per allocated FPGA. The specification is \
         execution engine-specific. If no FPGAs were allocated, then the value must be an empty \
         array.",
    ),
    (
        TASK_FIELD_DISKS,
        "A Map[String, Int] with one entry for each disk mount point. The key is the mount point \
         and the value is the initial amount of disk space allocated, in bytes. The execution \
         engine must, at a minimum, provide one entry for each disk mount point requested, but \
         may provide more. The amount of disk space available for a given mount point may \
         increase during the lifetime of the task (e.g., autoscaling volumes provided by some \
         cloud services).",
    ),
    (
        TASK_FIELD_ATTEMPT,
        "The current task attempt. The value must be 0 the first time the task is executed, and \
         incremented by 1 each time the task is retried (if any).",
    ),
    (
        TASK_FIELD_END_TIME,
        "An Int? whose value is the time by which the task must be completed, as a Unix time \
         stamp. A value of 0 means that the execution engine does not impose a time limit. A \
         value of None means that the execution engine cannot determine whether the runtime of \
         the task is limited. A positive value is a guarantee that the task will be preempted at \
         the specified time, but is not a guarantee that the task won't be preempted earlier.",
    ),
    (
        TASK_FIELD_RETURN_CODE,
        "An Int? whose value is initially None and is set to the value of the command's return \
         code. The value is only guaranteed to be defined in the output section.",
    ),
    (
        TASK_FIELD_META,
        "An Object containing a copy of the task's meta section, or the empty Object if there is \
         no meta section or if it is empty.",
    ),
    (
        TASK_FIELD_PARAMETER_META,
        "An Object containing a copy of the task's parameter_meta section, or the empty Object if \
         there is no parameter_meta section or if it is empty.",
    ),
    (
        TASK_FIELD_EXT,
        "An Object containing execution engine-specific attributes, or the empty Object if there \
         aren't any. Members of ext should be considered optional. It is recommended to only \
         access a member of ext using string interpolation to avoid an error if it is not defined.",
    ),
];

/// The set of all valid runtime section keys and their descriptions.
pub const RUNTIME_KEYS: &[(&str, &str)] = &[
    (
        TASK_REQUIREMENT_CONTAINER,
        "Specifies the container image (e.g., Docker, Singularity) to use for the task.",
    ),
    (
        TASK_REQUIREMENT_CPU,
        "The number of CPU cores required for the task.",
    ),
    (
        TASK_REQUIREMENT_MEMORY,
        "The amount of memory required, specified as a string with units (e.g., '2 GiB').",
    ),
    (
        TASK_REQUIREMENT_DISKS,
        "Specifies the disk requirements for the task.",
    ),
    (TASK_REQUIREMENT_GPU, "Specifies GPU requirements."),
];

/// The set of all valid requirements section keys and their descriptions.
pub const REQUIREMENTS_KEY: &[(&str, &str)] = &[
    (
        TASK_REQUIREMENT_CONTAINER,
        "Specifies a list of allowed container images. Use `*` to allow any POSIX environment.",
    ),
    (
        TASK_REQUIREMENT_CPU,
        "The minimum number of CPU cores required.",
    ),
    (
        TASK_REQUIREMENT_MEMORY,
        "The minimum amount of memory required.",
    ),
    (TASK_REQUIREMENT_GPU, "The minimum GPU requirements."),
    (TASK_REQUIREMENT_FPGA, "The minimum FPGA requirements."),
    (TASK_REQUIREMENT_DISKS, "The minimum disk requirements."),
    (
        TASK_REQUIREMENT_MAX_RETRIES,
        "The maximum number of times the task can be retried.",
    ),
    (
        TASK_REQUIREMENT_RETURN_CODES,
        "A list of acceptable return codes from the command.",
    ),
];

/// The set of all valid task hints section keys and their descriptions.
pub const TASK_HINT_KEYS: &[(&str, &str)] = &[
    (
        TASK_HINT_DISKS,
        "A hint to the execution engine to mount disks with specific attributes. The value of \
         this hint can be a String with a specification that applies to all mount points, or a \
         Map with the key being the mount point and the value being a String with the \
         specification for that mount point.",
    ),
    (
        TASK_HINT_GPU,
        "A hint to the execution engine to provision hardware accelerators with specific \
         attributes. Accelerator specifications are left intentionally vague as they are \
         primarily intended to be used in the context of a specific compute environment.",
    ),
    (
        TASK_HINT_FPGA,
        "A hint to the execution engine to provision hardware accelerators with specific \
         attributes. Accelerator specifications are left intentionally vague as they are \
         primarily intended to be used in the context of a specific compute environment.",
    ),
    (
        TASK_HINT_INPUTS,
        "Provides input-specific hints. Each key must refer to a parameter defined in the task's \
         input section. A key may also used dotted notation to refer to a specific member of a \
         struct input.",
    ),
    (
        TASK_HINT_LOCALIZATION_OPTIONAL,
        "A hint to the execution engine about whether the File inputs for this task need to be \
         localized prior to executing the task. The value of this hint is a Boolean for which \
         true indicates that the contents of the File inputs may be streamed on demand.",
    ),
    (
        TASK_HINT_MAX_CPU,
        "A hint to the execution engine that the task expects to use no more than the specified \
         number of CPUs. The value of this hint has the same specification as requirements.cpu.",
    ),
    (
        TASK_HINT_MAX_MEMORY,
        "A hint to the execution engine that the task expects to use no more than the specified \
         amount of memory. The value of this hint has the same specification as \
         requirements.memory.",
    ),
    (
        TASK_HINT_OUTPUTS,
        "Provides output-specific hints. Each key must refer to a parameter defined in the task's \
         output section. A key may also use dotted notation to refer to a specific member of a \
         struct output.",
    ),
    (
        TASK_HINT_SHORT_TASK,
        "A hint to the execution engine about the expected duration of this task. The value of \
         this hint is a Boolean for which true indicates that that this task is not expected to \
         take long to execute, which the execution engine can interpret as permission to optimize \
         the execution of the task.",
    ),
];

/// The name of the `name` task variable field.
pub const TASK_FIELD_NAME: &str = "name";
/// The name of the `id` task variable field.
pub const TASK_FIELD_ID: &str = "id";
/// The name of the `container` task variable field.
pub const TASK_FIELD_CONTAINER: &str = "container";
/// The name of the `cpu` task variable field.
pub const TASK_FIELD_CPU: &str = "cpu";
/// The name of the `memory` task variable field.
pub const TASK_FIELD_MEMORY: &str = "memory";
/// The name of the `attempt` task variable field.
pub const TASK_FIELD_ATTEMPT: &str = "attempt";
/// The name of the `gpu` task variable field.
pub const TASK_FIELD_GPU: &str = "gpu";
/// The name of the `fpga` task variable field.
pub const TASK_FIELD_FPGA: &str = "fpga";
/// The name of the `disks` task variable field.
pub const TASK_FIELD_DISKS: &str = "disks";
/// The name of the `end_time` task variable field.
pub const TASK_FIELD_END_TIME: &str = "end_time";
/// The name of the `return_code` task variable field.
pub const TASK_FIELD_RETURN_CODE: &str = "return_code";
/// The name of the `meta` task variable field.
pub const TASK_FIELD_META: &str = "meta";
/// The name of the `parameter_meta` task variable field.
pub const TASK_FIELD_PARAMETER_META: &str = "parameter_meta";
/// The name of the `ext` task variable field.
pub const TASK_FIELD_EXT: &str = "ext";

/// The name of the `container` task requirement.
pub const TASK_REQUIREMENT_CONTAINER: &str = "container";
/// The alias of the `container` task requirement (i.e. `docker`).
pub const TASK_REQUIREMENT_CONTAINER_ALIAS: &str = "docker";
/// The name of the `cpu` task requirement.
pub const TASK_REQUIREMENT_CPU: &str = "cpu";
/// The name of the `disks` task requirement.
pub const TASK_REQUIREMENT_DISKS: &str = "disks";
/// The name of the `gpu` task requirement.
pub const TASK_REQUIREMENT_GPU: &str = "gpu";
/// The name of the `fpga` task requirement.
pub const TASK_REQUIREMENT_FPGA: &str = "fpga";
/// The name of the `max_retries` task requirement.
pub const TASK_REQUIREMENT_MAX_RETRIES: &str = "max_retries";
/// The alias of the `max_retries` task requirement (i.e. `maxRetries``).
pub const TASK_REQUIREMENT_MAX_RETRIES_ALIAS: &str = "maxRetries";
/// The name of the `memory` task requirement.
pub const TASK_REQUIREMENT_MEMORY: &str = "memory";
/// The name of the `return_codes` task requirement.
pub const TASK_REQUIREMENT_RETURN_CODES: &str = "return_codes";
/// The alias of the `return_codes` task requirement (i.e. `returnCodes`).
pub const TASK_REQUIREMENT_RETURN_CODES_ALIAS: &str = "returnCodes";

/// The name of the `disks` task hint.
pub const TASK_HINT_DISKS: &str = "disks";
/// The name of the `gpu` task hint.
pub const TASK_HINT_GPU: &str = "gpu";
/// The name of the `fpga` task hint.
pub const TASK_HINT_FPGA: &str = "fpga";
/// The name of the `inputs` task hint.
pub const TASK_HINT_INPUTS: &str = "inputs";
/// The name of the `localization_optional` task hint.
pub const TASK_HINT_LOCALIZATION_OPTIONAL: &str = "localization_optional";
/// The alias of the `localization_optional` task hint (i.e.
/// `localizationOptional`).
pub const TASK_HINT_LOCALIZATION_OPTIONAL_ALIAS: &str = "localizationOptional";
/// The name of the `max_cpu` task hint.
pub const TASK_HINT_MAX_CPU: &str = "max_cpu";
/// The alias of the `max_cpu` task hint (i.e. `maxCpu`).
pub const TASK_HINT_MAX_CPU_ALIAS: &str = "maxCpu";
/// The name of the `max_memory` task hint.
pub const TASK_HINT_MAX_MEMORY: &str = "max_memory";
/// The alias of the `max_memory` task hin (e.g. `maxMemory`).
pub const TASK_HINT_MAX_MEMORY_ALIAS: &str = "maxMemory";
/// The name of the `outputs` task hint.
pub const TASK_HINT_OUTPUTS: &str = "outputs";
/// The name of the `short_task` task hint.
pub const TASK_HINT_SHORT_TASK: &str = "short_task";
/// The alias of the `short_task` task hint (e.g. `shortTask`).
pub const TASK_HINT_SHORT_TASK_ALIAS: &str = "shortTask";

/// Unescapes command text.
fn unescape_command_text(s: &str, heredoc: bool, buffer: &mut String) {
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.peek() {
                Some('\\') | Some('~') => {
                    buffer.push(chars.next().unwrap());
                }
                Some('>') if heredoc => {
                    buffer.push(chars.next().unwrap());
                }
                Some('$') | Some('}') if !heredoc => {
                    buffer.push(chars.next().unwrap());
                }
                _ => {
                    buffer.push('\\');
                }
            },
            _ => {
                buffer.push(c);
            }
        }
    }
}

/// Represents a task definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskDefinition<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> TaskDefinition<N> {
    /// Gets the name of the task.
    pub fn name(&self) -> Ident<N::Token> {
        self.token().expect("task should have a name")
    }

    /// Gets the items of the task.
    pub fn items(&self) -> impl Iterator<Item = TaskItem<N>> + use<'_, N> {
        TaskItem::children(&self.0)
    }

    /// Gets the input section of the task.
    pub fn input(&self) -> Option<InputSection<N>> {
        self.child()
    }

    /// Gets the output section of the task.
    pub fn output(&self) -> Option<OutputSection<N>> {
        self.child()
    }

    /// Gets the command section of the task.
    pub fn command(&self) -> Option<CommandSection<N>> {
        self.child()
    }

    /// Gets the requirements sections of the task.
    pub fn requirements(&self) -> Option<RequirementsSection<N>> {
        self.child()
    }

    /// Gets the hints section of the task.
    pub fn hints(&self) -> Option<TaskHintsSection<N>> {
        self.child()
    }

    /// Gets the runtime section of the task.
    pub fn runtime(&self) -> Option<RuntimeSection<N>> {
        self.child()
    }

    /// Gets the metadata section of the task.
    pub fn metadata(&self) -> Option<MetadataSection<N>> {
        self.child()
    }

    /// Gets the parameter section of the task.
    pub fn parameter_metadata(&self) -> Option<ParameterMetadataSection<N>> {
        self.child()
    }

    /// Gets the private declarations of the task.
    pub fn declarations(&self) -> impl Iterator<Item = BoundDecl<N>> + use<'_, N> {
        self.children()
    }

    /// Writes a Markdown formatted description of the task.
    pub fn markdown_description(&self, f: &mut impl fmt::Write) -> fmt::Result {
        writeln!(f, "```wdl\ntask {}\n```\n---", self.name().text())?;

        if let Some(meta) = self.metadata() {
            if let Some(desc) = meta.items().find(|i| i.name().text() == "description") {
                if let MetadataValue::String(s) = desc.value() {
                    if let Some(text) = s.text() {
                        writeln!(f, "{}\n", text.text())?;
                    }
                }
            }
        }

        write_input_section(f, self.input().as_ref(), self.parameter_metadata().as_ref())?;
        write_output_section(
            f,
            self.output().as_ref(),
            self.parameter_metadata().as_ref(),
        )?;

        Ok(())
    }
}

impl<N: TreeNode> AstNode<N> for TaskDefinition<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::TaskDefinitionNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::TaskDefinitionNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents an item in a task definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskItem<N: TreeNode = SyntaxNode> {
    /// The item is an input section.
    Input(InputSection<N>),
    /// The item is an output section.
    Output(OutputSection<N>),
    /// The item is a command section.
    Command(CommandSection<N>),
    /// The item is a requirements section.
    Requirements(RequirementsSection<N>),
    /// The item is a task hints section.
    Hints(TaskHintsSection<N>),
    /// The item is a runtime section.
    Runtime(RuntimeSection<N>),
    /// The item is a metadata section.
    Metadata(MetadataSection<N>),
    /// The item is a parameter meta section.
    ParameterMetadata(ParameterMetadataSection<N>),
    /// The item is a private bound declaration.
    Declaration(BoundDecl<N>),
}

impl<N: TreeNode> TaskItem<N> {
    /// Returns whether or not the given syntax kind can be cast to
    /// [`TaskItem`].
    pub fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::InputSectionNode
                | SyntaxKind::OutputSectionNode
                | SyntaxKind::CommandSectionNode
                | SyntaxKind::RequirementsSectionNode
                | SyntaxKind::TaskHintsSectionNode
                | SyntaxKind::RuntimeSectionNode
                | SyntaxKind::MetadataSectionNode
                | SyntaxKind::ParameterMetadataSectionNode
                | SyntaxKind::BoundDeclNode
        )
    }

    /// Casts the given node to [`TaskItem`].
    ///
    /// Returns `None` if the node cannot be cast.
    pub fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::InputSectionNode => Some(Self::Input(
                InputSection::cast(inner).expect("input section to cast"),
            )),
            SyntaxKind::OutputSectionNode => Some(Self::Output(
                OutputSection::cast(inner).expect("output section to cast"),
            )),
            SyntaxKind::CommandSectionNode => Some(Self::Command(
                CommandSection::cast(inner).expect("command section to cast"),
            )),
            SyntaxKind::RequirementsSectionNode => Some(Self::Requirements(
                RequirementsSection::cast(inner).expect("requirements section to cast"),
            )),
            SyntaxKind::RuntimeSectionNode => Some(Self::Runtime(
                RuntimeSection::cast(inner).expect("runtime section to cast"),
            )),
            SyntaxKind::MetadataSectionNode => Some(Self::Metadata(
                MetadataSection::cast(inner).expect("metadata section to cast"),
            )),
            SyntaxKind::ParameterMetadataSectionNode => Some(Self::ParameterMetadata(
                ParameterMetadataSection::cast(inner).expect("parameter metadata section to cast"),
            )),
            SyntaxKind::TaskHintsSectionNode => Some(Self::Hints(
                TaskHintsSection::cast(inner).expect("task hints section to cast"),
            )),
            SyntaxKind::BoundDeclNode => Some(Self::Declaration(
                BoundDecl::cast(inner).expect("bound decl to cast"),
            )),
            _ => None,
        }
    }

    /// Gets a reference to the inner node.
    pub fn inner(&self) -> &N {
        match self {
            Self::Input(element) => element.inner(),
            Self::Output(element) => element.inner(),
            Self::Command(element) => element.inner(),
            Self::Requirements(element) => element.inner(),
            Self::Hints(element) => element.inner(),
            Self::Runtime(element) => element.inner(),
            Self::Metadata(element) => element.inner(),
            Self::ParameterMetadata(element) => element.inner(),
            Self::Declaration(element) => element.inner(),
        }
    }

    /// Attempts to get a reference to the inner [`InputSection`].
    ///
    /// * If `self` is a [`TaskItem::Input`], then a reference to the inner
    ///   [`InputSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_input_section(&self) -> Option<&InputSection<N>> {
        match self {
            Self::Input(s) => Some(s),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`InputSection`].
    ///
    /// * If `self` is a [`TaskItem::Input`], then the inner [`InputSection`] is
    ///   returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_input_section(self) -> Option<InputSection<N>> {
        match self {
            Self::Input(s) => Some(s),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`OutputSection`].
    ///
    /// * If `self` is a [`TaskItem::Output`], then a reference to the inner
    ///   [`OutputSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_output_section(&self) -> Option<&OutputSection<N>> {
        match self {
            Self::Output(s) => Some(s),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`OutputSection`].
    ///
    /// * If `self` is a [`TaskItem::Output`], then the inner [`OutputSection`]
    ///   is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_output_section(self) -> Option<OutputSection<N>> {
        match self {
            Self::Output(s) => Some(s),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`CommandSection`].
    ///
    /// * If `self` is a [`TaskItem::Command`], then a reference to the inner
    ///   [`CommandSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_command_section(&self) -> Option<&CommandSection<N>> {
        match self {
            Self::Command(s) => Some(s),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`CommandSection`].
    ///
    /// * If `self` is a [`TaskItem::Command`], then the inner
    ///   [`CommandSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_command_section(self) -> Option<CommandSection<N>> {
        match self {
            Self::Command(s) => Some(s),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`RequirementsSection`].
    ///
    /// * If `self` is a [`TaskItem::Requirements`], then a reference to the
    ///   inner [`RequirementsSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_requirements_section(&self) -> Option<&RequirementsSection<N>> {
        match self {
            Self::Requirements(s) => Some(s),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner
    /// [`RequirementsSection`].
    ///
    /// * If `self` is a [`TaskItem::Requirements`], then the inner
    ///   [`RequirementsSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_requirements_section(self) -> Option<RequirementsSection<N>> {
        match self {
            Self::Requirements(s) => Some(s),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`TaskHintsSection`].
    ///
    /// * If `self` is a [`TaskItem::Hints`], then a reference to the inner
    ///   [`TaskHintsSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_hints_section(&self) -> Option<&TaskHintsSection<N>> {
        match self {
            Self::Hints(s) => Some(s),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`TaskHintsSection`].
    ///
    /// * If `self` is a [`TaskItem::Hints`], then the inner
    ///   [`TaskHintsSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_hints_section(self) -> Option<TaskHintsSection<N>> {
        match self {
            Self::Hints(s) => Some(s),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`RuntimeSection`].
    ///
    /// * If `self` is a [`TaskItem::Runtime`], then a reference to the inner
    ///   [`RuntimeSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_runtime_section(&self) -> Option<&RuntimeSection<N>> {
        match self {
            Self::Runtime(s) => Some(s),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`RuntimeSection`].
    ///
    /// * If `self` is a [`TaskItem::Runtime`], then the inner
    ///   [`RuntimeSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_runtime_section(self) -> Option<RuntimeSection<N>> {
        match self {
            Self::Runtime(s) => Some(s),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`MetadataSection`].
    ///
    /// * If `self` is a [`TaskItem::Metadata`], then a reference to the inner
    ///   [`MetadataSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_metadata_section(&self) -> Option<&MetadataSection<N>> {
        match self {
            Self::Metadata(s) => Some(s),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`MetadataSection`].
    ///
    /// * If `self` is a [`TaskItem::Metadata`], then the inner
    ///   [`MetadataSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_metadata_section(self) -> Option<MetadataSection<N>> {
        match self {
            Self::Metadata(s) => Some(s),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`ParameterMetadataSection`].
    ///
    /// * If `self` is a [`TaskItem::ParameterMetadata`], then a reference to
    ///   the inner [`ParameterMetadataSection`] is returned wrapped in
    ///   [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_parameter_metadata_section(&self) -> Option<&ParameterMetadataSection<N>> {
        match self {
            Self::ParameterMetadata(s) => Some(s),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner
    /// [`ParameterMetadataSection`].
    ///
    /// * If `self` is a [`TaskItem::ParameterMetadata`], then the inner
    ///   [`ParameterMetadataSection`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_parameter_metadata_section(self) -> Option<ParameterMetadataSection<N>> {
        match self {
            Self::ParameterMetadata(s) => Some(s),
            _ => None,
        }
    }

    /// Attempts to get a reference to the inner [`BoundDecl`].
    ///
    /// * If `self` is a [`TaskItem::Declaration`], then a reference to the
    ///   inner [`BoundDecl`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_declaration(&self) -> Option<&BoundDecl<N>> {
        match self {
            Self::Declaration(d) => Some(d),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`BoundDecl`].
    ///
    /// * If `self` is a [`TaskItem::Declaration`], then the inner [`BoundDecl`]
    ///   is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_declaration(self) -> Option<BoundDecl<N>> {
        match self {
            Self::Declaration(d) => Some(d),
            _ => None,
        }
    }

    /// Finds the first child that can be cast to a [`TaskItem`].
    pub fn child(node: &N) -> Option<Self> {
        node.children().find_map(Self::cast)
    }

    /// Finds all children that can be cast to a [`TaskItem`].
    pub fn children(node: &N) -> impl Iterator<Item = Self> + use<'_, N> {
        node.children().filter_map(Self::cast)
    }
}

/// Represents the parent of a section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SectionParent<N: TreeNode = SyntaxNode> {
    /// The parent is a task.
    Task(TaskDefinition<N>),
    /// The parent is a workflow.
    Workflow(WorkflowDefinition<N>),
    /// The parent is a struct.
    Struct(StructDefinition<N>),
}

impl<N: TreeNode> SectionParent<N> {
    /// Returns whether or not the given syntax kind can be cast to
    /// [`SectionParent`].
    pub fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::TaskDefinitionNode
                | SyntaxKind::WorkflowDefinitionNode
                | SyntaxKind::StructDefinitionNode
        )
    }

    /// Casts the given node to [`SectionParent`].
    ///
    /// Returns `None` if the node cannot be cast.
    pub fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::TaskDefinitionNode => Some(Self::Task(
                TaskDefinition::cast(inner).expect("task definition to cast"),
            )),
            SyntaxKind::WorkflowDefinitionNode => Some(Self::Workflow(
                WorkflowDefinition::cast(inner).expect("workflow definition to cast"),
            )),
            SyntaxKind::StructDefinitionNode => Some(Self::Struct(
                StructDefinition::cast(inner).expect("struct definition to cast"),
            )),
            _ => None,
        }
    }

    /// Gets a reference to the inner node.
    pub fn inner(&self) -> &N {
        match self {
            Self::Task(element) => element.inner(),
            Self::Workflow(element) => element.inner(),
            Self::Struct(element) => element.inner(),
        }
    }

    /// Gets the name of the section parent.
    pub fn name(&self) -> Ident<N::Token> {
        match self {
            Self::Task(t) => t.name(),
            Self::Workflow(w) => w.name(),
            Self::Struct(s) => s.name(),
        }
    }

    /// Attempts to get a reference to the inner [`TaskDefinition`].
    ///
    /// * If `self` is a [`SectionParent::Task`], then a reference to the inner
    ///   [`TaskDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_task(&self) -> Option<&TaskDefinition<N>> {
        match self {
            Self::Task(task) => Some(task),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`TaskDefinition`].
    ///
    /// * If `self` is a [`SectionParent::Task`], then the inner
    ///   [`TaskDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_task(self) -> Option<TaskDefinition<N>> {
        match self {
            Self::Task(task) => Some(task),
            _ => None,
        }
    }

    /// Unwraps to a task definition.
    ///
    /// # Panics
    ///
    /// Panics if it is not a task definition.
    pub fn unwrap_task(self) -> TaskDefinition<N> {
        match self {
            Self::Task(task) => task,
            _ => panic!("not a task definition"),
        }
    }

    /// Attempts to get a reference to the inner [`WorkflowDefinition`].
    ///
    /// * If `self` is a [`SectionParent::Workflow`], then a reference to the
    ///   inner [`WorkflowDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_workflow(&self) -> Option<&WorkflowDefinition<N>> {
        match self {
            Self::Workflow(workflow) => Some(workflow),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`WorkflowDefinition`].
    ///
    /// * If `self` is a [`SectionParent::Workflow`], then the inner
    ///   [`WorkflowDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_workflow(self) -> Option<WorkflowDefinition<N>> {
        match self {
            Self::Workflow(workflow) => Some(workflow),
            _ => None,
        }
    }

    /// Unwraps to a workflow definition.
    ///
    /// # Panics
    ///
    /// Panics if it is not a workflow definition.
    pub fn unwrap_workflow(self) -> WorkflowDefinition<N> {
        match self {
            Self::Workflow(workflow) => workflow,
            _ => panic!("not a workflow definition"),
        }
    }

    /// Attempts to get a reference to the inner [`StructDefinition`].
    ///
    /// * If `self` is a [`SectionParent::Struct`], then a reference to the
    ///   inner [`StructDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn as_struct(&self) -> Option<&StructDefinition<N>> {
        match self {
            Self::Struct(r#struct) => Some(r#struct),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`StructDefinition`].
    ///
    /// * If `self` is a [`SectionParent::Struct`], then the inner
    ///   [`StructDefinition`] is returned wrapped in [`Some`].
    /// * Else, [`None`] is returned.
    pub fn into_struct(self) -> Option<StructDefinition<N>> {
        match self {
            Self::Struct(r#struct) => Some(r#struct),
            _ => None,
        }
    }

    /// Unwraps to a struct definition.
    ///
    /// # Panics
    ///
    /// Panics if it is not a struct definition.
    pub fn unwrap_struct(self) -> StructDefinition<N> {
        match self {
            Self::Struct(def) => def,
            _ => panic!("not a struct definition"),
        }
    }

    /// Finds the first child that can be cast to a [`SectionParent`].
    pub fn child(node: &N) -> Option<Self> {
        node.children().find_map(Self::cast)
    }

    /// Finds all children that can be cast to a [`SectionParent`].
    pub fn children(node: &N) -> impl Iterator<Item = Self> + use<'_, N> {
        node.children().filter_map(Self::cast)
    }
}

/// Represents an input section in a task or workflow definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputSection<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> InputSection<N> {
    /// Gets the declarations of the input section.
    pub fn declarations(&self) -> impl Iterator<Item = Decl<N>> + use<'_, N> {
        Decl::children(&self.0)
    }

    /// Gets the parent of the input section.
    pub fn parent(&self) -> SectionParent<N> {
        SectionParent::cast(self.0.parent().expect("should have a parent"))
            .expect("parent should cast")
    }
}

impl<N: TreeNode> AstNode<N> for InputSection<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::InputSectionNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::InputSectionNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents an output section in a task or workflow definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputSection<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> OutputSection<N> {
    /// Gets the declarations of the output section.
    pub fn declarations(&self) -> impl Iterator<Item = BoundDecl<N>> + use<'_, N> {
        self.children()
    }

    /// Gets the parent of the output section.
    pub fn parent(&self) -> SectionParent<N> {
        SectionParent::cast(self.0.parent().expect("should have a parent"))
            .expect("parent should cast")
    }
}

impl<N: TreeNode> AstNode<N> for OutputSection<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::OutputSectionNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::OutputSectionNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// A command part stripped of leading whitespace.
///
/// Placeholders are not changed and are copied as is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StrippedCommandPart<N: TreeNode = SyntaxNode> {
    /// A text part.
    Text(String),
    /// A placeholder part.
    Placeholder(Placeholder<N>),
}

/// Represents a command section in a task definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandSection<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> CommandSection<N> {
    /// Gets whether or not the command section is a heredoc command.
    pub fn is_heredoc(&self) -> bool {
        self.token::<OpenHeredoc<N::Token>>().is_some()
    }

    /// Gets the parts of the command.
    pub fn parts(&self) -> impl Iterator<Item = CommandPart<N>> + use<'_, N> {
        self.0.children_with_tokens().filter_map(CommandPart::cast)
    }

    /// Gets the command text if the command is not interpolated (i.e. has no
    /// placeholders).
    ///
    /// Returns `None` if the command is interpolated, as interpolated commands
    /// cannot be represented as a single span of text.
    pub fn text(&self) -> Option<CommandText<N::Token>> {
        let mut parts = self.parts();
        if let Some(CommandPart::Text(text)) = parts.next() {
            if parts.next().is_none() {
                return Some(text);
            }
        }

        None
    }

    /// Counts the leading whitespace of the command.
    ///
    /// If the command has mixed indentation, this will return None.
    pub fn count_whitespace(&self) -> Option<usize> {
        let mut min_leading_spaces = usize::MAX;
        let mut min_leading_tabs = usize::MAX;
        let mut parsing_leading_whitespace = false; // init to false so that the first line is skipped

        let mut leading_spaces = 0;
        let mut leading_tabs = 0;
        for part in self.parts() {
            match part {
                CommandPart::Text(text) => {
                    for c in text.text().chars() {
                        match c {
                            ' ' if parsing_leading_whitespace => {
                                leading_spaces += 1;
                            }
                            '\t' if parsing_leading_whitespace => {
                                leading_tabs += 1;
                            }
                            '\n' => {
                                parsing_leading_whitespace = true;
                                leading_spaces = 0;
                                leading_tabs = 0;
                            }
                            '\r' => {}
                            _ => {
                                if parsing_leading_whitespace {
                                    parsing_leading_whitespace = false;
                                    if leading_spaces == 0 && leading_tabs == 0 {
                                        min_leading_spaces = 0;
                                        min_leading_tabs = 0;
                                        continue;
                                    }
                                    if leading_spaces < min_leading_spaces && leading_spaces > 0 {
                                        min_leading_spaces = leading_spaces;
                                    }
                                    if leading_tabs < min_leading_tabs && leading_tabs > 0 {
                                        min_leading_tabs = leading_tabs;
                                    }
                                }
                            }
                        }
                    }
                    // The last line is intentionally skipped.
                }
                CommandPart::Placeholder(_) => {
                    if parsing_leading_whitespace {
                        parsing_leading_whitespace = false;
                        if leading_spaces == 0 && leading_tabs == 0 {
                            min_leading_spaces = 0;
                            min_leading_tabs = 0;
                            continue;
                        }
                        if leading_spaces < min_leading_spaces && leading_spaces > 0 {
                            min_leading_spaces = leading_spaces;
                        }
                        if leading_tabs < min_leading_tabs && leading_tabs > 0 {
                            min_leading_tabs = leading_tabs;
                        }
                    }
                }
            }
        }

        // Check for no indentation or all whitespace, in which case we're done
        if (min_leading_spaces == 0 && min_leading_tabs == 0)
            || (min_leading_spaces == usize::MAX && min_leading_tabs == usize::MAX)
        {
            return Some(0);
        }

        // Check for mixed indentation
        if (min_leading_spaces > 0 && min_leading_spaces != usize::MAX)
            && (min_leading_tabs > 0 && min_leading_tabs != usize::MAX)
        {
            return None;
        }

        // Exactly one of the two will be equal to usize::MAX because it never appeared.
        // The other will be the number of leading spaces or tabs to strip.
        let final_leading_whitespace = if min_leading_spaces < min_leading_tabs {
            min_leading_spaces
        } else {
            min_leading_tabs
        };

        Some(final_leading_whitespace)
    }

    /// Strips leading whitespace from the command.
    ///
    /// If the command has mixed indentation, this will return `None`.
    pub fn strip_whitespace(&self) -> Option<Vec<StrippedCommandPart<N>>> {
        let mut result = Vec::new();
        let heredoc = self.is_heredoc();
        for part in self.parts() {
            match part {
                CommandPart::Text(text) => {
                    let mut s = String::new();
                    unescape_command_text(text.text(), heredoc, &mut s);
                    result.push(StrippedCommandPart::Text(s));
                }
                CommandPart::Placeholder(p) => {
                    result.push(StrippedCommandPart::Placeholder(p));
                }
            }
        }

        // Trim the first line
        let mut whole_first_line_trimmed = false;
        if let Some(StrippedCommandPart::Text(text)) = result.first_mut() {
            let end_of_first_line = text.find('\n').map(|p| p + 1).unwrap_or(text.len());
            let line = &text[..end_of_first_line];
            let len = line.len() - line.trim_start().len();
            whole_first_line_trimmed = len == line.len();
            text.replace_range(..len, "");
        }

        // Trim the last line
        if let Some(StrippedCommandPart::Text(text)) = result.last_mut() {
            if let Some(index) = text.rfind(|c| !matches!(c, ' ' | '\t')) {
                text.truncate(index + 1);
            } else {
                text.clear();
            }

            if text.ends_with('\n') {
                text.pop();
            }

            if text.ends_with('\r') {
                text.pop();
            }
        }

        // Return immediately if command contains mixed indentation
        let num_stripped_chars = self.count_whitespace()?;

        // If there is no leading whitespace, we're done
        if num_stripped_chars == 0 {
            return Some(result);
        }

        // Finally, strip the leading whitespace on each line
        // This is done in place using the `replace_range` method; the method will
        // internally do moves without allocations
        let mut strip_leading_whitespace = whole_first_line_trimmed;
        for part in &mut result {
            match part {
                StrippedCommandPart::Text(text) => {
                    let mut offset = 0;
                    while let Some(next) = text[offset..].find('\n') {
                        let next = next + offset;
                        if offset > 0 {
                            strip_leading_whitespace = true;
                        }

                        if !strip_leading_whitespace {
                            offset = next + 1;
                            continue;
                        }

                        let line = &text[offset..next];
                        let line = line.strip_suffix('\r').unwrap_or(line);
                        let len = line.len().min(num_stripped_chars);
                        text.replace_range(offset..offset + len, "");
                        offset = next + 1 - len;
                    }

                    // Replace any remaining text
                    if strip_leading_whitespace || offset > 0 {
                        let line = &text[offset..];
                        let line = line.strip_suffix('\r').unwrap_or(line);
                        let len = line.len().min(num_stripped_chars);
                        text.replace_range(offset..offset + len, "");
                    }
                }
                StrippedCommandPart::Placeholder(_) => {
                    strip_leading_whitespace = false;
                }
            }
        }

        Some(result)
    }

    /// Gets the parent of the command section.
    pub fn parent(&self) -> SectionParent<N> {
        SectionParent::cast(self.0.parent().expect("should have a parent"))
            .expect("parent should cast")
    }
}

impl<N: TreeNode> AstNode<N> for CommandSection<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CommandSectionNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::CommandSectionNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a textual part of a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandText<T: TreeToken = SyntaxToken>(T);

impl<T: TreeToken> CommandText<T> {
    /// Unescapes the command text to the given buffer.
    ///
    /// When `heredoc` is true, only heredoc escape sequences are allowed.
    ///
    /// Otherwise, brace command sequences are accepted.
    pub fn unescape_to(&self, heredoc: bool, buffer: &mut String) {
        unescape_command_text(self.text(), heredoc, buffer);
    }
}

impl<T: TreeToken> AstToken<T> for CommandText<T> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralCommandText
    }

    fn cast(inner: T) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralCommandText => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &T {
        &self.0
    }
}

/// Represents a part of a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandPart<N: TreeNode = SyntaxNode> {
    /// A textual part of the command.
    Text(CommandText<N::Token>),
    /// A placeholder encountered in the command.
    Placeholder(Placeholder<N>),
}

impl<N: TreeNode> CommandPart<N> {
    /// Unwraps the command part into text.
    ///
    /// # Panics
    ///
    /// Panics if the command part is not text.
    pub fn unwrap_text(self) -> CommandText<N::Token> {
        match self {
            Self::Text(text) => text,
            _ => panic!("not string text"),
        }
    }

    /// Unwraps the command part into a placeholder.
    ///
    /// # Panics
    ///
    /// Panics if the command part is not a placeholder.
    pub fn unwrap_placeholder(self) -> Placeholder<N> {
        match self {
            Self::Placeholder(p) => p,
            _ => panic!("not a placeholder"),
        }
    }

    /// Casts the given [`NodeOrToken`] to [`CommandPart`].
    ///
    /// Returns `None` if it cannot case cannot be cast.
    fn cast(element: NodeOrToken<N, N::Token>) -> Option<Self> {
        match element {
            NodeOrToken::Node(n) => Some(Self::Placeholder(Placeholder::cast(n)?)),
            NodeOrToken::Token(t) => Some(Self::Text(CommandText::cast(t)?)),
        }
    }
}

/// Represents a requirements section in a task definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequirementsSection<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> RequirementsSection<N> {
    /// Gets the items in the requirements section.
    pub fn items(&self) -> impl Iterator<Item = RequirementsItem<N>> + use<'_, N> {
        self.children()
    }

    /// Gets the parent of the requirements section.
    pub fn parent(&self) -> SectionParent<N> {
        SectionParent::cast(self.0.parent().expect("should have a parent"))
            .expect("parent should cast")
    }

    /// Gets the `container` item as a
    /// [`Container`](requirements::item::Container) (if it exists).
    pub fn container(&self) -> Option<requirements::item::Container<N>> {
        // NOTE: validation should ensure that, at most, one `container` item exists in
        // the `requirements` section.
        self.child()
    }
}

impl<N: TreeNode> AstNode<N> for RequirementsSection<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::RequirementsSectionNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::RequirementsSectionNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents an item in a requirements section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequirementsItem<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> RequirementsItem<N> {
    /// Gets the name of the requirements item.
    pub fn name(&self) -> Ident<N::Token> {
        self.token().expect("expected an item name")
    }

    /// Gets the expression of the requirements item.
    pub fn expr(&self) -> Expr<N> {
        Expr::child(&self.0).expect("expected an item expression")
    }

    /// Consumes `self` and attempts to cast the requirements item to a
    /// [`Container`](requirements::item::Container).
    pub fn into_container(self) -> Option<requirements::item::Container<N>> {
        requirements::item::Container::try_from(self).ok()
    }
}

impl<N: TreeNode> AstNode<N> for RequirementsItem<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::RequirementsItemNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::RequirementsItemNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a hints section in a task definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskHintsSection<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> TaskHintsSection<N> {
    /// Gets the items in the hints section.
    pub fn items(&self) -> impl Iterator<Item = TaskHintsItem<N>> + use<'_, N> {
        self.children()
    }

    /// Gets the parent of the hints section.
    pub fn parent(&self) -> TaskDefinition<N> {
        TaskDefinition::cast(self.0.parent().expect("should have a parent"))
            .expect("parent should cast")
    }
}

impl<N: TreeNode> AstNode<N> for TaskHintsSection<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::TaskHintsSectionNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::TaskHintsSectionNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents an item in a task hints section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskHintsItem<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> TaskHintsItem<N> {
    /// Gets the name of the hints item.
    pub fn name(&self) -> Ident<N::Token> {
        self.token().expect("expected an item name")
    }

    /// Gets the expression of the hints item.
    pub fn expr(&self) -> Expr<N> {
        Expr::child(&self.0).expect("expected an item expression")
    }
}

impl<N: TreeNode> AstNode<N> for TaskHintsItem<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::TaskHintsItemNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::TaskHintsItemNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a runtime section in a task definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeSection<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> RuntimeSection<N> {
    /// Gets the items in the runtime section.
    pub fn items(&self) -> impl Iterator<Item = RuntimeItem<N>> + use<'_, N> {
        self.children()
    }

    /// Gets the parent of the runtime section.
    pub fn parent(&self) -> SectionParent<N> {
        SectionParent::cast(self.0.parent().expect("should have a parent"))
            .expect("parent should cast")
    }

    /// Gets the `container` item as a [`Container`](runtime::item::Container)
    /// (if it exists).
    pub fn container(&self) -> Option<runtime::item::Container<N>> {
        // NOTE: validation should ensure that, at most, one `container`/`docker` item
        // exists in the `runtime` section.
        self.child()
    }
}

impl<N: TreeNode> AstNode<N> for RuntimeSection<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::RuntimeSectionNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::RuntimeSectionNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents an item in a runtime section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeItem<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> RuntimeItem<N> {
    /// Gets the name of the runtime item.
    pub fn name(&self) -> Ident<N::Token> {
        self.token().expect("expected an item name")
    }

    /// Gets the expression of the runtime item.
    pub fn expr(&self) -> Expr<N> {
        Expr::child(&self.0).expect("expected an item expression")
    }

    /// Consumes `self` and attempts to cast the runtime item to a
    /// [`Container`](runtime::item::Container).
    pub fn into_container(self) -> Option<runtime::item::Container<N>> {
        runtime::item::Container::try_from(self).ok()
    }
}

impl<N: TreeNode> AstNode<N> for RuntimeItem<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::RuntimeItemNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::RuntimeItemNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a metadata section in a task or workflow definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataSection<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> MetadataSection<N> {
    /// Gets the items of the metadata section.
    pub fn items(&self) -> impl Iterator<Item = MetadataObjectItem<N>> + use<'_, N> {
        self.children()
    }

    /// Gets the parent of the metadata section.
    pub fn parent(&self) -> SectionParent<N> {
        SectionParent::cast(self.0.parent().expect("should have a parent"))
            .expect("parent should cast")
    }
}

impl<N: TreeNode> AstNode<N> for MetadataSection<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::MetadataSectionNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::MetadataSectionNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a metadata object item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataObjectItem<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> MetadataObjectItem<N> {
    /// Gets the name of the item.
    pub fn name(&self) -> Ident<N::Token> {
        self.token().expect("expected a name")
    }

    /// Gets the value of the item.
    pub fn value(&self) -> MetadataValue<N> {
        self.child().expect("expected a value")
    }
}

impl<N: TreeNode> AstNode<N> for MetadataObjectItem<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::MetadataObjectItemNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::MetadataObjectItemNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a metadata value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetadataValue<N: TreeNode = SyntaxNode> {
    /// The value is a literal boolean.
    Boolean(LiteralBoolean<N>),
    /// The value is a literal integer.
    Integer(LiteralInteger<N>),
    /// The value is a literal float.
    Float(LiteralFloat<N>),
    /// The value is a literal string.
    String(LiteralString<N>),
    /// The value is a literal null.
    Null(LiteralNull<N>),
    /// The value is a metadata object.
    Object(MetadataObject<N>),
    /// The value is a metadata array.
    Array(MetadataArray<N>),
}

impl<N: TreeNode> MetadataValue<N> {
    /// Unwraps the metadata value into a boolean.
    ///
    /// # Panics
    ///
    /// Panics if the metadata value is not a boolean.
    pub fn unwrap_boolean(self) -> LiteralBoolean<N> {
        match self {
            Self::Boolean(b) => b,
            _ => panic!("not a boolean"),
        }
    }

    /// Unwraps the metadata value into an integer.
    ///
    /// # Panics
    ///
    /// Panics if the metadata value is not an integer.
    pub fn unwrap_integer(self) -> LiteralInteger<N> {
        match self {
            Self::Integer(i) => i,
            _ => panic!("not an integer"),
        }
    }

    /// Unwraps the metadata value into a float.
    ///
    /// # Panics
    ///
    /// Panics if the metadata value is not a float.
    pub fn unwrap_float(self) -> LiteralFloat<N> {
        match self {
            Self::Float(f) => f,
            _ => panic!("not a float"),
        }
    }

    /// Unwraps the metadata value into a string.
    ///
    /// # Panics
    ///
    /// Panics if the metadata value is not a string.
    pub fn unwrap_string(self) -> LiteralString<N> {
        match self {
            Self::String(s) => s,
            _ => panic!("not a string"),
        }
    }

    /// Unwraps the metadata value into a null.
    ///
    /// # Panics
    ///
    /// Panics if the metadata value is not a null.
    pub fn unwrap_null(self) -> LiteralNull<N> {
        match self {
            Self::Null(n) => n,
            _ => panic!("not a null"),
        }
    }

    /// Unwraps the metadata value into an object.
    ///
    /// # Panics
    ///
    /// Panics if the metadata value is not an object.
    pub fn unwrap_object(self) -> MetadataObject<N> {
        match self {
            Self::Object(o) => o,
            _ => panic!("not an object"),
        }
    }

    /// Unwraps the metadata value into an array.
    ///
    /// # Panics
    ///
    /// Panics if the metadata value is not an array.
    pub fn unwrap_array(self) -> MetadataArray<N> {
        match self {
            Self::Array(a) => a,
            _ => panic!("not an array"),
        }
    }
}

impl<N: TreeNode> AstNode<N> for MetadataValue<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::LiteralBooleanNode
                | SyntaxKind::LiteralIntegerNode
                | SyntaxKind::LiteralFloatNode
                | SyntaxKind::LiteralStringNode
                | SyntaxKind::LiteralNullNode
                | SyntaxKind::MetadataObjectNode
                | SyntaxKind::MetadataArrayNode
        )
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralBooleanNode => Some(Self::Boolean(LiteralBoolean(inner))),
            SyntaxKind::LiteralIntegerNode => Some(Self::Integer(LiteralInteger(inner))),
            SyntaxKind::LiteralFloatNode => Some(Self::Float(LiteralFloat(inner))),
            SyntaxKind::LiteralStringNode => Some(Self::String(LiteralString(inner))),
            SyntaxKind::LiteralNullNode => Some(Self::Null(LiteralNull(inner))),
            SyntaxKind::MetadataObjectNode => Some(Self::Object(MetadataObject(inner))),
            SyntaxKind::MetadataArrayNode => Some(Self::Array(MetadataArray(inner))),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        match self {
            Self::Boolean(b) => &b.0,
            Self::Integer(i) => &i.0,
            Self::Float(f) => &f.0,
            Self::String(s) => &s.0,
            Self::Null(n) => &n.0,
            Self::Object(o) => &o.0,
            Self::Array(a) => &a.0,
        }
    }
}

/// Represents a literal null.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralNull<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> AstNode<N> for LiteralNull<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralNullNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::LiteralNullNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a metadata object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataObject<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> MetadataObject<N> {
    /// Gets the items of the metadata object.
    pub fn items(&self) -> impl Iterator<Item = MetadataObjectItem<N>> + use<'_, N> {
        self.children()
    }
}

impl<N: TreeNode> AstNode<N> for MetadataObject<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::MetadataObjectNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::MetadataObjectNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a metadata array.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataArray<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> MetadataArray<N> {
    /// Gets the elements of the metadata array.
    pub fn elements(&self) -> impl Iterator<Item = MetadataValue<N>> + use<'_, N> {
        self.children()
    }
}

impl<N: TreeNode> AstNode<N> for MetadataArray<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::MetadataArrayNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::MetadataArrayNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

/// Represents a parameter metadata section in a task or workflow definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterMetadataSection<N: TreeNode = SyntaxNode>(N);

impl<N: TreeNode> ParameterMetadataSection<N> {
    /// Gets the items of the parameter metadata section.
    pub fn items(&self) -> impl Iterator<Item = MetadataObjectItem<N>> + use<'_, N> {
        self.children()
    }

    /// Gets the parent of the parameter metadata section.
    pub fn parent(&self) -> SectionParent<N> {
        SectionParent::cast(self.0.parent().expect("should have a parent"))
            .expect("parent should cast")
    }
}

impl<N: TreeNode> AstNode<N> for ParameterMetadataSection<N> {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ParameterMetadataSectionNode
    }

    fn cast(inner: N) -> Option<Self> {
        match inner.kind() {
            SyntaxKind::ParameterMetadataSectionNode => Some(Self(inner)),
            _ => None,
        }
    }

    fn inner(&self) -> &N {
        &self.0
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::Document;

    #[test]
    fn tasks() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.2

task test {
    input {
        String name
    }

    output {
        String output = stdout()
    }

    command <<<
        printf "hello, ~{name}!
    >>>

    requirements {
        container: "baz/qux"
    }

    hints {
        foo: "bar"
    }

    runtime {
        container: "foo/bar"
    }

    meta {
        description: "a test"
        foo: null
    }

    parameter_meta {
        name: {
            help: "a name to greet"
        }
    }

    String x = "private"
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name().text(), "test");

        // Task input
        let input = tasks[0].input().expect("should have an input section");
        assert_eq!(input.parent().unwrap_task().name().text(), "test");
        let decls: Vec<_> = input.declarations().collect();
        assert_eq!(decls.len(), 1);
        assert_eq!(
            decls[0].clone().unwrap_unbound_decl().ty().to_string(),
            "String"
        );
        assert_eq!(decls[0].clone().unwrap_unbound_decl().name().text(), "name");

        // Task output
        let output = tasks[0].output().expect("should have an output section");
        assert_eq!(output.parent().unwrap_task().name().text(), "test");
        let decls: Vec<_> = output.declarations().collect();
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].ty().to_string(), "String");
        assert_eq!(decls[0].name().text(), "output");
        assert_eq!(decls[0].expr().unwrap_call().target().text(), "stdout");

        // Task command
        let command = tasks[0].command().expect("should have a command section");
        assert_eq!(command.parent().name().text(), "test");
        assert!(command.is_heredoc());
        let parts: Vec<_> = command.parts().collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(
            parts[0].clone().unwrap_text().text(),
            "\n        printf \"hello, "
        );
        assert_eq!(
            parts[1]
                .clone()
                .unwrap_placeholder()
                .expr()
                .unwrap_name_ref()
                .name()
                .text(),
            "name"
        );
        assert_eq!(parts[2].clone().unwrap_text().text(), "!\n    ");

        // Task requirements
        let requirements = tasks[0]
            .requirements()
            .expect("should have a requirements section");
        assert_eq!(requirements.parent().name().text(), "test");
        let items: Vec<_> = requirements.items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name().text(), TASK_REQUIREMENT_CONTAINER);
        assert_eq!(
            items[0]
                .expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "baz/qux"
        );

        // Task hints
        let hints = tasks[0].hints().expect("should have a hints section");
        assert_eq!(hints.parent().name().text(), "test");
        let items: Vec<_> = hints.items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name().text(), "foo");
        assert_eq!(
            items[0]
                .expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "bar"
        );

        // Task runtimes
        let runtime = tasks[0].runtime().expect("should have a runtime section");
        assert_eq!(runtime.parent().name().text(), "test");
        let items: Vec<_> = runtime.items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name().text(), TASK_REQUIREMENT_CONTAINER);
        assert_eq!(
            items[0]
                .expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "foo/bar"
        );

        // Task metadata
        let metadata = tasks[0].metadata().expect("should have a metadata section");
        assert_eq!(metadata.parent().unwrap_task().name().text(), "test");
        let items: Vec<_> = metadata.items().collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name().text(), "description");
        assert_eq!(
            items[0].value().unwrap_string().text().unwrap().text(),
            "a test"
        );

        // Second metadata
        assert_eq!(items[1].name().text(), "foo");
        items[1].value().unwrap_null();

        // Task parameter metadata
        let param_meta = tasks[0]
            .parameter_metadata()
            .expect("should have a parameter metadata section");
        assert_eq!(param_meta.parent().unwrap_task().name().text(), "test");
        let items: Vec<_> = param_meta.items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name().text(), "name");
        let items: Vec<_> = items[0].value().unwrap_object().items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name().text(), "help");
        assert_eq!(
            items[0].value().unwrap_string().text().unwrap().text(),
            "a name to greet"
        );

        // Task declarations
        let decls: Vec<_> = tasks[0].declarations().collect();
        assert_eq!(decls.len(), 1);

        // First task declaration
        assert_eq!(decls[0].ty().to_string(), "String");
        assert_eq!(decls[0].name().text(), "x");
        assert_eq!(
            decls[0]
                .expr()
                .unwrap_literal()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "private"
        );
    }

    #[test]
    fn whitespace_stripping_without_interpolation() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.2

task test {
    command <<<
        echo "hello"
        echo "world"
        echo \
            "goodbye"
    >>>
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);

        let command = tasks[0].command().expect("should have a command section");

        let stripped = command.strip_whitespace().unwrap();

        assert_eq!(stripped.len(), 1);
        let text = match &stripped[0] {
            StrippedCommandPart::Text(text) => text,
            _ => panic!("expected text"),
        };
        assert_eq!(
            text,
            "echo \"hello\"\necho \"world\"\necho \\\n    \"goodbye\""
        );
    }

    #[test]
    fn whitespace_stripping_with_interpolation() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.2

task test {
    input {
        String name
        Boolean flag
    }

    command <<<
        echo "hello, ~{
if flag
then name
               else "Jerry"
    }!"
    >>>
}
    "#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);

        let command = tasks[0].command().expect("should have a command section");

        let stripped = command.strip_whitespace().unwrap();
        assert_eq!(stripped.len(), 3);
        let text = match &stripped[0] {
            StrippedCommandPart::Text(text) => text,
            _ => panic!("expected text"),
        };
        assert_eq!(text, "echo \"hello, ");

        let _placeholder = match &stripped[1] {
            StrippedCommandPart::Placeholder(p) => p,
            _ => panic!("expected placeholder"),
        };
        // not testing anything with the placeholder, just making sure it's there

        let text = match &stripped[2] {
            StrippedCommandPart::Text(text) => text,
            _ => panic!("expected text"),
        };
        assert_eq!(text, "!\"");
    }

    #[test]
    fn whitespace_stripping_when_interpolation_starts_line() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.2

task test {
    input {
      Int placeholder
    }

    command <<<
            # other weird whitspace
      ~{placeholder} "$trailing_pholder" ~{placeholder}
      ~{placeholder} somecommand.py "$leading_pholder"
    >>>
}
"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);

        let command = tasks[0].command().expect("should have a command section");

        let stripped = command.strip_whitespace().unwrap();
        assert_eq!(stripped.len(), 7);
        let text = match &stripped[0] {
            StrippedCommandPart::Text(text) => text,
            _ => panic!("expected text"),
        };
        assert_eq!(text, "      # other weird whitspace\n");

        let _placeholder = match &stripped[1] {
            StrippedCommandPart::Placeholder(p) => p,
            _ => panic!("expected placeholder"),
        };
        // not testing anything with the placeholder, just making sure it's there

        let text = match &stripped[2] {
            StrippedCommandPart::Text(text) => text,
            _ => panic!("expected text"),
        };
        assert_eq!(text, " \"$trailing_pholder\" ");

        let _placeholder = match &stripped[3] {
            StrippedCommandPart::Placeholder(p) => p,
            _ => panic!("expected placeholder"),
        };
        // not testing anything with the placeholder, just making sure it's there

        let text = match &stripped[4] {
            StrippedCommandPart::Text(text) => text,
            _ => panic!("expected text"),
        };
        assert_eq!(text, "\n");

        let _placeholder = match &stripped[5] {
            StrippedCommandPart::Placeholder(p) => p,
            _ => panic!("expected placeholder"),
        };
        // not testing anything with the placeholder, just making sure it's there

        let text = match &stripped[6] {
            StrippedCommandPart::Text(text) => text,
            _ => panic!("expected text"),
        };
        assert_eq!(text, " somecommand.py \"$leading_pholder\"");
    }

    #[test]
    fn whitespace_stripping_when_command_is_empty() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.2

task test {
    command <<<>>>
}
    "#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);

        let command = tasks[0].command().expect("should have a command section");

        let stripped = command.strip_whitespace().unwrap();
        assert_eq!(stripped.len(), 0);
    }

    #[test]
    fn whitespace_stripping_when_command_is_one_line_of_whitespace() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.2

task test {
    command <<<     >>>
}
    "#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);

        let command = tasks[0].command().expect("should have a command section");

        let stripped = command.strip_whitespace().unwrap();
        assert_eq!(stripped.len(), 1);
        let text = match &stripped[0] {
            StrippedCommandPart::Text(text) => text,
            _ => panic!("expected text"),
        };
        assert_eq!(text, "");
    }

    #[test]
    fn whitespace_stripping_when_command_is_one_newline() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.2

task test {
    command <<<
    >>>
}
    "#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);

        let command = tasks[0].command().expect("should have a command section");

        let stripped = command.strip_whitespace().unwrap();
        assert_eq!(stripped.len(), 1);
        let text = match &stripped[0] {
            StrippedCommandPart::Text(text) => text,
            _ => panic!("expected text"),
        };
        assert_eq!(text, "");
    }

    #[test]
    fn whitespace_stripping_when_command_is_a_blank_line() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.2

task test {
    command <<<

    >>>
}
    "#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);

        let command = tasks[0].command().expect("should have a command section");

        let stripped = command.strip_whitespace().unwrap();
        assert_eq!(stripped.len(), 1);
        let text = match &stripped[0] {
            StrippedCommandPart::Text(text) => text,
            _ => panic!("expected text"),
        };
        assert_eq!(text, "");
    }

    #[test]
    fn whitespace_stripping_when_command_is_a_blank_line_with_spaces() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.2

task test {
    command <<<
    
    >>>
}
    "#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);

        let command = tasks[0].command().expect("should have a command section");

        let stripped = command.strip_whitespace().unwrap();
        assert_eq!(stripped.len(), 1);
        let text = match &stripped[0] {
            StrippedCommandPart::Text(text) => text,
            _ => panic!("expected text"),
        };
        assert_eq!(text, "    ");
    }

    #[test]
    fn whitespace_stripping_with_mixed_indentation() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.2

task test {
    command <<<
        echo "hello"
			echo "world"
        echo \
            "goodbye"
    >>>
        }"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);

        let command = tasks[0].command().expect("should have a command section");

        let stripped = command.strip_whitespace();
        assert!(stripped.is_none());
    }

    #[test]
    fn whitespace_stripping_with_funky_indentation() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.2

task test {
    command <<<
    echo "hello"
        echo "world"
    echo \
            "goodbye"
                >>>
        }"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);

        let command = tasks[0].command().expect("should have a command section");

        let stripped = command.strip_whitespace().unwrap();
        assert_eq!(stripped.len(), 1);
        let text = match &stripped[0] {
            StrippedCommandPart::Text(text) => text,
            _ => panic!("expected text"),
        };
        assert_eq!(
            text,
            "echo \"hello\"\n    echo \"world\"\necho \\\n        \"goodbye\""
        );
    }

    /// Regression test for issue [#268](https://github.com/stjude-rust-labs/wdl/issues/268).
    #[test]
    fn whitespace_stripping_with_content_on_first_line() {
        let (document, diagnostics) = Document::parse(
            r#"
version 1.2

task test {
    command <<<      weird stuff $firstlinelint
            # other weird whitespace
      somecommand.py $line120 ~{placeholder}
    >>>
        }"#,
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);

        let command = tasks[0].command().expect("should have a command section");

        let stripped = command.strip_whitespace().unwrap();
        assert_eq!(stripped.len(), 3);
        let text = match &stripped[0] {
            StrippedCommandPart::Text(text) => text,
            _ => panic!("expected text"),
        };
        assert_eq!(
            text,
            "weird stuff $firstlinelint\n      # other weird whitespace\nsomecommand.py $line120 "
        );

        let _placeholder = match &stripped[1] {
            StrippedCommandPart::Placeholder(p) => p,
            _ => panic!("expected placeholder"),
        };
        // not testing anything with the placeholder, just making sure it's there

        let text = match &stripped[2] {
            StrippedCommandPart::Text(text) => text,
            _ => panic!("expected text"),
        };
        assert_eq!(text, "");
    }

    #[test]
    fn whitespace_stripping_on_windows() {
        let (document, diagnostics) = Document::parse(
            "version 1.2\r\ntask test {\r\n    command <<<\r\n        echo \"hello\"\r\n    \
             >>>\r\n}\r\n",
        );

        assert!(diagnostics.is_empty());
        let ast = document.ast();
        let ast = ast.as_v1().expect("should be a V1 AST");
        let tasks: Vec<_> = ast.tasks().collect();
        assert_eq!(tasks.len(), 1);

        let command = tasks[0].command().expect("should have a command section");
        let stripped = command.strip_whitespace().unwrap();
        assert_eq!(stripped.len(), 1);
        let text = match &stripped[0] {
            StrippedCommandPart::Text(text) => text,
            _ => panic!("expected text"),
        };
        assert_eq!(text, "echo \"hello\"");
    }
}

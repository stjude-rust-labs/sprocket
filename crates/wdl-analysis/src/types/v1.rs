//! Type conversion helpers for a V1 AST.

use std::fmt;
use std::fmt::Write;
use std::sync::Arc;
use std::sync::LazyLock;

use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
use wdl_ast::Severity;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::TreeNode;
use wdl_ast::TreeToken;
use wdl_ast::v1;
use wdl_ast::v1::AccessExpr;
use wdl_ast::v1::CallExpr;
use wdl_ast::v1::Expr;
use wdl_ast::v1::IfExpr;
use wdl_ast::v1::IndexExpr;
use wdl_ast::v1::LiteralArray;
use wdl_ast::v1::LiteralExpr;
use wdl_ast::v1::LiteralHints;
use wdl_ast::v1::LiteralInput;
use wdl_ast::v1::LiteralMap;
use wdl_ast::v1::LiteralMapItem;
use wdl_ast::v1::LiteralObject;
use wdl_ast::v1::LiteralOutput;
use wdl_ast::v1::LiteralPair;
use wdl_ast::v1::LiteralStruct;
use wdl_ast::v1::LogicalAndExpr;
use wdl_ast::v1::LogicalNotExpr;
use wdl_ast::v1::LogicalOrExpr;
use wdl_ast::v1::NegationExpr;
use wdl_ast::v1::Placeholder;
use wdl_ast::v1::PlaceholderOption;
use wdl_ast::v1::StringPart;
use wdl_ast::v1::TASK_FIELD_ATTEMPT;
use wdl_ast::v1::TASK_FIELD_CONTAINER;
use wdl_ast::v1::TASK_FIELD_CPU;
use wdl_ast::v1::TASK_FIELD_DISKS;
use wdl_ast::v1::TASK_FIELD_END_TIME;
use wdl_ast::v1::TASK_FIELD_EXT;
use wdl_ast::v1::TASK_FIELD_FPGA;
use wdl_ast::v1::TASK_FIELD_GPU;
use wdl_ast::v1::TASK_FIELD_ID;
use wdl_ast::v1::TASK_FIELD_MAX_RETRIES;
use wdl_ast::v1::TASK_FIELD_MEMORY;
use wdl_ast::v1::TASK_FIELD_META;
use wdl_ast::v1::TASK_FIELD_NAME;
use wdl_ast::v1::TASK_FIELD_PARAMETER_META;
use wdl_ast::v1::TASK_FIELD_PREVIOUS;
use wdl_ast::v1::TASK_FIELD_RETURN_CODE;
use wdl_ast::v1::TASK_HINT_DISKS;
use wdl_ast::v1::TASK_HINT_FPGA;
use wdl_ast::v1::TASK_HINT_GPU;
use wdl_ast::v1::TASK_HINT_INPUTS;
use wdl_ast::v1::TASK_HINT_LOCALIZATION_OPTIONAL;
use wdl_ast::v1::TASK_HINT_LOCALIZATION_OPTIONAL_ALIAS;
use wdl_ast::v1::TASK_HINT_MAX_CPU;
use wdl_ast::v1::TASK_HINT_MAX_CPU_ALIAS;
use wdl_ast::v1::TASK_HINT_MAX_MEMORY;
use wdl_ast::v1::TASK_HINT_MAX_MEMORY_ALIAS;
use wdl_ast::v1::TASK_HINT_OUTPUTS;
use wdl_ast::v1::TASK_HINT_SHORT_TASK;
use wdl_ast::v1::TASK_HINT_SHORT_TASK_ALIAS;
use wdl_ast::v1::TASK_REQUIREMENT_CONTAINER;
use wdl_ast::v1::TASK_REQUIREMENT_CONTAINER_ALIAS;
use wdl_ast::v1::TASK_REQUIREMENT_CPU;
use wdl_ast::v1::TASK_REQUIREMENT_DISKS;
use wdl_ast::v1::TASK_REQUIREMENT_FPGA;
use wdl_ast::v1::TASK_REQUIREMENT_GPU;
use wdl_ast::v1::TASK_REQUIREMENT_MAX_RETRIES;
use wdl_ast::v1::TASK_REQUIREMENT_MAX_RETRIES_ALIAS;
use wdl_ast::v1::TASK_REQUIREMENT_MEMORY;
use wdl_ast::v1::TASK_REQUIREMENT_RETURN_CODES;
use wdl_ast::v1::TASK_REQUIREMENT_RETURN_CODES_ALIAS;
use wdl_ast::version::V1;

use super::ArrayType;
use super::CompoundType;
use super::HiddenType;
use super::MapType;
use super::Optional;
use super::PairType;
use super::PrimitiveType;
use super::StructType;
use super::Type;
use super::TypeNameResolver;
use crate::SyntaxNodeExt;
use crate::UNNECESSARY_FUNCTION_CALL;
use crate::config::DiagnosticsConfig;
use crate::diagnostics::Io;
use crate::diagnostics::ambiguous_argument;
use crate::diagnostics::argument_type_mismatch;
use crate::diagnostics::cannot_access;
use crate::diagnostics::cannot_coerce_to_string;
use crate::diagnostics::cannot_index;
use crate::diagnostics::comparison_mismatch;
use crate::diagnostics::if_conditional_mismatch;
use crate::diagnostics::index_type_mismatch;
use crate::diagnostics::invalid_placeholder_option;
use crate::diagnostics::invalid_regex_pattern;
use crate::diagnostics::logical_and_mismatch;
use crate::diagnostics::logical_not_mismatch;
use crate::diagnostics::logical_or_mismatch;
use crate::diagnostics::map_key_not_primitive;
use crate::diagnostics::missing_struct_members;
use crate::diagnostics::multiple_type_mismatch;
use crate::diagnostics::negation_mismatch;
use crate::diagnostics::no_common_type;
use crate::diagnostics::not_a_pair_accessor;
use crate::diagnostics::not_a_struct;
use crate::diagnostics::not_a_struct_member;
use crate::diagnostics::not_a_previous_requirements_member;
use crate::diagnostics::not_a_task_member;
use crate::diagnostics::numeric_mismatch;
use crate::diagnostics::string_concat_mismatch;
use crate::diagnostics::too_few_arguments;
use crate::diagnostics::too_many_arguments;
use crate::diagnostics::type_mismatch;
use crate::diagnostics::unknown_call_io;
use crate::diagnostics::unknown_function;
use crate::diagnostics::unknown_task_io;
use crate::diagnostics::unnecessary_function_call;
use crate::diagnostics::unsupported_function;
use crate::document::Task;
use crate::stdlib::FunctionBindError;
use crate::stdlib::MAX_PARAMETERS;
use crate::stdlib::STDLIB;
use crate::types::Coercible;

/// Gets the type of a `task` variable member for pre-evaluation contexts.
///
/// This is used in requirements, hints, and runtime sections where
/// `task.previous` and `task.attempt` are available.
///
/// Returns [`None`] if the given member name is unknown.
pub fn task_member_type_pre_evaluation(name: &str) -> Option<Type> {
    match name {
        n if n == TASK_FIELD_NAME || n == TASK_FIELD_ID => Some(PrimitiveType::String.into()),
        n if n == TASK_FIELD_ATTEMPT => Some(PrimitiveType::Integer.into()),
        n if n == TASK_FIELD_PREVIOUS => Some(Type::Hidden(HiddenType::PreviousRequirements)),
        _ => None,
    }
}

/// Gets the type of a `task` variable member for post-evaluation contexts.
///
/// This is used in command and output sections where all task fields are
/// available.
///
/// Returns [`None`] if the given member name is unknown.
pub fn task_member_type_post_evaluation(
    version: SupportedVersion,
    name: &str,
) -> Option<Type> {
    match name {
        n if n == TASK_FIELD_NAME || n == TASK_FIELD_ID => Some(PrimitiveType::String.into()),
        n if n == TASK_FIELD_CONTAINER => Some(Type::from(PrimitiveType::String).optional()),
        n if n == TASK_FIELD_CPU => Some(PrimitiveType::Float.into()),
        n if n == TASK_FIELD_MEMORY || n == TASK_FIELD_ATTEMPT => {
            Some(PrimitiveType::Integer.into())
        }
        n if n == TASK_FIELD_GPU || n == TASK_FIELD_FPGA => {
            Some(STDLIB.array_string_type().clone())
        }
        n if n == TASK_FIELD_DISKS => Some(STDLIB.map_string_int_type().clone()),
        n if n == TASK_FIELD_END_TIME || n == TASK_FIELD_RETURN_CODE => {
            Some(Type::from(PrimitiveType::Integer).optional())
        }
        n if n == TASK_FIELD_META || n == TASK_FIELD_PARAMETER_META || n == TASK_FIELD_EXT => {
            Some(Type::Object)
        }
        n if version >= SupportedVersion::V1(V1::Three) && n == TASK_FIELD_PREVIOUS => {
            Some(Type::Hidden(HiddenType::PreviousRequirements))
        }
        _ => None,
    }
}

/// Gets the type of a `task.previous` member.
///
/// Returns [`None`] if the given member name is unknown.
pub fn previous_requirements_member_type(name: &str) -> Option<Type> {
    match name {
        n if n == TASK_FIELD_MEMORY => Some(Type::from(PrimitiveType::Integer).optional()),
        n if n == TASK_FIELD_CPU => Some(Type::from(PrimitiveType::Float).optional()),
        n if n == TASK_FIELD_CONTAINER => Some(Type::from(PrimitiveType::String).optional()),
        n if n == TASK_FIELD_GPU => Some(Type::from(PrimitiveType::Boolean).optional()),
        n if n == TASK_FIELD_FPGA => Some(Type::from(PrimitiveType::Boolean).optional()),
        n if n == TASK_FIELD_DISKS => Some(
            Type::Compound(
                CompoundType::Array(ArrayType::new(PrimitiveType::String)),
                false,
            )
            .optional(),
        ),
        n if n == TASK_FIELD_MAX_RETRIES => Some(Type::from(PrimitiveType::Integer).optional()),
        _ => None,
    }
}

/// Gets the types of a task requirement.
///
/// Returns a slice of types or `None` if the given name is not a requirement.
pub fn task_requirement_types(version: SupportedVersion, name: &str) -> Option<&'static [Type]> {
    /// The types for the `container` requirement.
    static CONTAINER_TYPES: LazyLock<Box<[Type]>> = LazyLock::new(|| {
        Box::new([
            PrimitiveType::String.into(),
            STDLIB.array_string_type().clone(),
        ])
    });
    /// The types for the `cpu` requirement.
    const CPU_TYPES: &[Type] = &[
        Type::Primitive(PrimitiveType::Integer, false),
        Type::Primitive(PrimitiveType::Float, false),
    ];
    /// The types for the `memory` requirement.
    const MEMORY_TYPES: &[Type] = &[
        Type::Primitive(PrimitiveType::Integer, false),
        Type::Primitive(PrimitiveType::String, false),
    ];
    /// The types for the `gpu` requirement.
    const GPU_TYPES: &[Type] = &[Type::Primitive(PrimitiveType::Boolean, false)];
    /// The types for the `fpga` requirement.
    const FPGA_TYPES: &[Type] = &[Type::Primitive(PrimitiveType::Boolean, false)];
    /// The types for the `disks` requirement.
    static DISKS_TYPES: LazyLock<Box<[Type]>> = LazyLock::new(|| {
        Box::new([
            PrimitiveType::Integer.into(),
            PrimitiveType::String.into(),
            STDLIB.array_string_type().clone(),
        ])
    });
    /// The types for the `max_retries` requirement.
    const MAX_RETRIES_TYPES: &[Type] = &[Type::Primitive(PrimitiveType::Integer, false)];
    /// The types for the `return_codes` requirement.
    static RETURN_CODES_TYPES: LazyLock<Box<[Type]>> = LazyLock::new(|| {
        Box::new([
            PrimitiveType::Integer.into(),
            PrimitiveType::String.into(),
            STDLIB.array_int_type().clone(),
        ])
    });

    match name {
        n if n == TASK_REQUIREMENT_CONTAINER || n == TASK_REQUIREMENT_CONTAINER_ALIAS => {
            Some(&CONTAINER_TYPES)
        }
        n if n == TASK_REQUIREMENT_CPU => Some(CPU_TYPES),
        n if n == TASK_REQUIREMENT_DISKS => Some(&DISKS_TYPES),
        n if n == TASK_REQUIREMENT_GPU => Some(GPU_TYPES),
        n if version >= SupportedVersion::V1(V1::Two) && n == TASK_REQUIREMENT_FPGA => {
            Some(FPGA_TYPES)
        }
        n if version >= SupportedVersion::V1(V1::Two) && n == TASK_REQUIREMENT_MAX_RETRIES => {
            Some(MAX_RETRIES_TYPES)
        }
        n if n == TASK_REQUIREMENT_MAX_RETRIES_ALIAS => Some(MAX_RETRIES_TYPES),
        n if n == TASK_REQUIREMENT_MEMORY => Some(MEMORY_TYPES),
        n if version >= SupportedVersion::V1(V1::Two) && n == TASK_REQUIREMENT_RETURN_CODES => {
            Some(&RETURN_CODES_TYPES)
        }
        n if n == TASK_REQUIREMENT_RETURN_CODES_ALIAS => Some(&RETURN_CODES_TYPES),
        _ => None,
    }
}

/// Gets the types of a task hint.
///
/// Returns a slice of types or `None` if the given name is not a reserved hint.
pub fn task_hint_types(
    version: SupportedVersion,
    name: &str,
    use_hidden_types: bool,
) -> Option<&'static [Type]> {
    /// The types for the `disks` hint.
    static DISKS_TYPES: LazyLock<Box<[Type]>> = LazyLock::new(|| {
        Box::new([
            PrimitiveType::String.into(),
            STDLIB.map_string_string_type().clone(),
        ])
    });
    /// The types for the `fpga` hint.
    const FPGA_TYPES: &[Type] = &[
        Type::Primitive(PrimitiveType::Integer, false),
        Type::Primitive(PrimitiveType::String, false),
    ];
    /// The types for the `gpu` hint.
    const GPU_TYPES: &[Type] = &[
        Type::Primitive(PrimitiveType::Integer, false),
        Type::Primitive(PrimitiveType::String, false),
    ];
    /// The types for the `inputs` hint.
    const INPUTS_TYPES: &[Type] = &[Type::Object];
    /// The types for the `inputs` hint (with hidden types).
    const INPUTS_HIDDEN_TYPES: &[Type] = &[Type::Hidden(HiddenType::Input)];
    /// The types for the `localization_optional` hint.
    const LOCALIZATION_OPTIONAL_TYPES: &[Type] = &[Type::Primitive(PrimitiveType::Boolean, false)];
    /// The types for the `max_cpu` hint.
    const MAX_CPU_TYPES: &[Type] = &[
        Type::Primitive(PrimitiveType::Integer, false),
        Type::Primitive(PrimitiveType::Float, false),
    ];
    /// The types for the `max_memory` hint.
    const MAX_MEMORY_TYPES: &[Type] = &[
        Type::Primitive(PrimitiveType::Integer, false),
        Type::Primitive(PrimitiveType::String, false),
    ];
    /// The types for the `outputs` hint.
    const OUTPUTS_TYPES: &[Type] = &[Type::Object];
    /// The types for the `outputs` hint (with hidden types).
    const OUTPUTS_HIDDEN_TYPES: &[Type] = &[Type::Hidden(HiddenType::Output)];
    /// The types for the `short_task` hint.
    const SHORT_TASK_TYPES: &[Type] = &[Type::Primitive(PrimitiveType::Boolean, false)];

    match name {
        n if n == TASK_HINT_DISKS => Some(&DISKS_TYPES),
        n if version >= SupportedVersion::V1(V1::Two) && n == TASK_HINT_FPGA => Some(FPGA_TYPES),
        n if n == TASK_HINT_GPU => Some(GPU_TYPES),
        n if use_hidden_types
            && version >= SupportedVersion::V1(V1::Two)
            && n == TASK_HINT_INPUTS =>
        {
            Some(INPUTS_HIDDEN_TYPES)
        }
        n if n == TASK_HINT_INPUTS => Some(INPUTS_TYPES),
        n if version >= SupportedVersion::V1(V1::Two) && n == TASK_HINT_LOCALIZATION_OPTIONAL => {
            Some(LOCALIZATION_OPTIONAL_TYPES)
        }
        n if n == TASK_HINT_LOCALIZATION_OPTIONAL_ALIAS => Some(LOCALIZATION_OPTIONAL_TYPES),
        n if version >= SupportedVersion::V1(V1::Two) && n == TASK_HINT_MAX_CPU => {
            Some(MAX_CPU_TYPES)
        }
        n if n == TASK_HINT_MAX_CPU_ALIAS => Some(MAX_CPU_TYPES),
        n if version >= SupportedVersion::V1(V1::Two) && n == TASK_HINT_MAX_MEMORY => {
            Some(MAX_MEMORY_TYPES)
        }
        n if n == TASK_HINT_MAX_MEMORY_ALIAS => Some(MAX_MEMORY_TYPES),
        n if use_hidden_types
            && version >= SupportedVersion::V1(V1::Two)
            && n == TASK_HINT_OUTPUTS =>
        {
            Some(OUTPUTS_HIDDEN_TYPES)
        }
        n if n == TASK_HINT_OUTPUTS => Some(OUTPUTS_TYPES),
        n if version >= SupportedVersion::V1(V1::Two) && n == TASK_HINT_SHORT_TASK => {
            Some(SHORT_TASK_TYPES)
        }
        n if n == TASK_HINT_SHORT_TASK_ALIAS => Some(SHORT_TASK_TYPES),
        _ => None,
    }
}

/// Represents a comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOperator {
    /// The `==` operator.
    Equality,
    /// The `!=` operator.
    Inequality,
    /// The `>` operator.
    Less,
    /// The `<=` operator.
    LessEqual,
    /// The `>` operator.
    Greater,
    /// The `>=` operator.
    GreaterEqual,
}

impl fmt::Display for ComparisonOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Equality => "==",
                Self::Inequality => "!=",
                Self::Less => "<",
                Self::LessEqual => "<=",
                Self::Greater => ">",
                Self::GreaterEqual => ">=",
            }
        )
    }
}

/// Represents a numeric operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericOperator {
    /// The `+` operator.
    Addition,
    /// The `-` operator.
    Subtraction,
    /// The `*` operator.
    Multiplication,
    /// The `/` operator.
    Division,
    /// The `%` operator.
    Modulo,
    /// The `**` operator.
    Exponentiation,
}

impl fmt::Display for NumericOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Addition => "addition",
                Self::Subtraction => "subtraction",
                Self::Multiplication => "multiplication",
                Self::Division => "division",
                Self::Modulo => "remainder",
                Self::Exponentiation => "exponentiation",
            }
        )
    }
}

/// Used to convert AST types into diagnostic types.
#[derive(Debug)]
pub struct AstTypeConverter<R>(R);

impl<R> AstTypeConverter<R>
where
    R: TypeNameResolver,
{
    /// Constructs a new AST type converter.
    pub fn new(resolver: R) -> Self {
        Self(resolver)
    }

    /// Converts a V1 AST type into an analysis type.
    ///
    /// If a type could not created, an error with the relevant diagnostic is
    /// returned.
    pub fn convert_type<N: TreeNode>(&mut self, ty: &v1::Type<N>) -> Result<Type, Diagnostic> {
        let optional = ty.is_optional();

        let ty: Type = match ty {
            v1::Type::Map(ty) => {
                let ty = self.convert_map_type(ty)?;
                ty.into()
            }
            v1::Type::Array(ty) => {
                let ty = self.convert_array_type(ty)?;
                ty.into()
            }
            v1::Type::Pair(ty) => {
                let ty = self.convert_pair_type(ty)?;
                ty.into()
            }
            v1::Type::Object(_) => Type::Object,
            v1::Type::Ref(r) => {
                let name = r.name();
                self.0.resolve(name.text(), name.span())?
            }
            v1::Type::Primitive(ty) => Type::Primitive(ty.kind().into(), false),
        };

        if optional { Ok(ty.optional()) } else { Ok(ty) }
    }

    /// Converts an AST array type to a diagnostic array type.
    ///
    /// If a type could not created, an error with the relevant diagnostic is
    /// returned.
    pub fn convert_array_type<N: TreeNode>(
        &mut self,
        ty: &v1::ArrayType<N>,
    ) -> Result<ArrayType, Diagnostic> {
        let element_type = self.convert_type(&ty.element_type())?;
        if ty.is_non_empty() {
            Ok(ArrayType::non_empty(element_type))
        } else {
            Ok(ArrayType::new(element_type))
        }
    }

    /// Converts an AST pair type into a diagnostic pair type.
    ///
    /// If a type could not created, an error with the relevant diagnostic is
    /// returned.
    pub fn convert_pair_type<N: TreeNode>(
        &mut self,
        ty: &v1::PairType<N>,
    ) -> Result<PairType, Diagnostic> {
        let (left_type, right_type) = ty.types();
        Ok(PairType::new(
            self.convert_type(&left_type)?,
            self.convert_type(&right_type)?,
        ))
    }

    /// Creates an AST map type into a diagnostic map type.
    ///
    /// If a type could not created, an error with the relevant diagnostic is
    /// returned.
    pub fn convert_map_type<N: TreeNode>(
        &mut self,
        ty: &v1::MapType<N>,
    ) -> Result<MapType, Diagnostic> {
        let (key_type, value_type) = ty.types();
        let optional = key_type.is_optional();
        Ok(MapType::new(
            Type::Primitive(key_type.kind().into(), optional),
            self.convert_type(&value_type)?,
        ))
    }

    /// Converts an AST struct definition into a struct type.
    ///
    /// If the type could not created, an error with the relevant diagnostic is
    /// returned.
    pub fn convert_struct_type<N: TreeNode>(
        &mut self,
        definition: &v1::StructDefinition<N>,
    ) -> Result<StructType, Diagnostic> {
        Ok(StructType {
            name: Arc::new(definition.name().text().to_string()),
            members: definition
                .members()
                .map(|d| Ok((d.name().text().to_string(), self.convert_type(&d.ty())?)))
                .collect::<Result<_, _>>()?,
        })
    }
}

impl From<v1::PrimitiveTypeKind> for PrimitiveType {
    fn from(value: v1::PrimitiveTypeKind) -> Self {
        match value {
            v1::PrimitiveTypeKind::Boolean => Self::Boolean,
            v1::PrimitiveTypeKind::Integer => Self::Integer,
            v1::PrimitiveTypeKind::Float => Self::Float,
            v1::PrimitiveTypeKind::String => Self::String,
            v1::PrimitiveTypeKind::File => Self::File,
            v1::PrimitiveTypeKind::Directory => Self::Directory,
        }
    }
}

/// Represents context to an expression type evaluator.
pub trait EvaluationContext {
    /// Gets the supported version of the document being evaluated.
    fn version(&self) -> SupportedVersion;

    /// Gets the type of the given name in scope.
    fn resolve_name(&self, name: &str, span: Span) -> Option<Type>;

    /// Resolves a type name to a type.
    fn resolve_type_name(&mut self, name: &str, span: Span) -> Result<Type, Diagnostic>;

    /// Gets the task associated with the evaluation context.
    ///
    /// This is only `Some` when evaluating a task `hints` section.
    fn task(&self) -> Option<&Task>;

    /// Gets the diagnostics configuration for the evaluation.
    fn diagnostics_config(&self) -> DiagnosticsConfig;

    /// Adds a diagnostic.
    fn add_diagnostic(&mut self, diagnostic: Diagnostic);
}

/// Represents an evaluator of expression types.
#[derive(Debug)]
pub struct ExprTypeEvaluator<'a, C> {
    /// The context for the evaluator.
    context: &'a mut C,
    /// The nested count of placeholder evaluation.
    ///
    /// This is incremented immediately before a placeholder expression is
    /// evaluated and decremented immediately after.
    ///
    /// If the count is non-zero, special evaluation behavior is enabled for
    /// string interpolation.
    placeholders: usize,
}

impl<'a, C: EvaluationContext> ExprTypeEvaluator<'a, C> {
    /// Constructs a new expression type evaluator.
    pub fn new(context: &'a mut C) -> Self {
        Self {
            context,
            placeholders: 0,
        }
    }

    /// Evaluates the type of the given expression in the given scope.
    ///
    /// Returns `None` if the type of the expression is indeterminate.
    pub fn evaluate_expr<N: TreeNode + SyntaxNodeExt>(&mut self, expr: &Expr<N>) -> Option<Type> {
        match expr {
            Expr::Literal(expr) => self.evaluate_literal_expr(expr),
            Expr::NameRef(r) => {
                let name = r.name();
                self.context.resolve_name(name.text(), name.span())
            }
            Expr::Parenthesized(expr) => self.evaluate_expr(&expr.expr()),
            Expr::If(expr) => self.evaluate_if_expr(expr),
            Expr::LogicalNot(expr) => self.evaluate_logical_not_expr(expr),
            Expr::Negation(expr) => self.evaluate_negation_expr(expr),
            Expr::LogicalOr(expr) => self.evaluate_logical_or_expr(expr),
            Expr::LogicalAnd(expr) => self.evaluate_logical_and_expr(expr),
            Expr::Equality(expr) => {
                let (lhs, rhs) = expr.operands();
                self.evaluate_comparison_expr(ComparisonOperator::Equality, &lhs, &rhs, expr.span())
            }
            Expr::Inequality(expr) => {
                let (lhs, rhs) = expr.operands();
                self.evaluate_comparison_expr(
                    ComparisonOperator::Inequality,
                    &lhs,
                    &rhs,
                    expr.span(),
                )
            }
            Expr::Less(expr) => {
                let (lhs, rhs) = expr.operands();
                self.evaluate_comparison_expr(ComparisonOperator::Less, &lhs, &rhs, expr.span())
            }
            Expr::LessEqual(expr) => {
                let (lhs, rhs) = expr.operands();
                self.evaluate_comparison_expr(
                    ComparisonOperator::LessEqual,
                    &lhs,
                    &rhs,
                    expr.span(),
                )
            }
            Expr::Greater(expr) => {
                let (lhs, rhs) = expr.operands();
                self.evaluate_comparison_expr(ComparisonOperator::Greater, &lhs, &rhs, expr.span())
            }
            Expr::GreaterEqual(expr) => {
                let (lhs, rhs) = expr.operands();
                self.evaluate_comparison_expr(
                    ComparisonOperator::GreaterEqual,
                    &lhs,
                    &rhs,
                    expr.span(),
                )
            }
            Expr::Addition(expr) => {
                let (lhs, rhs) = expr.operands();
                self.evaluate_numeric_expr(NumericOperator::Addition, expr.span(), &lhs, &rhs)
            }
            Expr::Subtraction(expr) => {
                let (lhs, rhs) = expr.operands();
                self.evaluate_numeric_expr(NumericOperator::Subtraction, expr.span(), &lhs, &rhs)
            }
            Expr::Multiplication(expr) => {
                let (lhs, rhs) = expr.operands();
                self.evaluate_numeric_expr(NumericOperator::Multiplication, expr.span(), &lhs, &rhs)
            }
            Expr::Division(expr) => {
                let (lhs, rhs) = expr.operands();
                self.evaluate_numeric_expr(NumericOperator::Division, expr.span(), &lhs, &rhs)
            }
            Expr::Modulo(expr) => {
                let (lhs, rhs) = expr.operands();
                self.evaluate_numeric_expr(NumericOperator::Modulo, expr.span(), &lhs, &rhs)
            }
            Expr::Exponentiation(expr) => {
                let (lhs, rhs) = expr.operands();
                self.evaluate_numeric_expr(NumericOperator::Exponentiation, expr.span(), &lhs, &rhs)
            }
            Expr::Call(expr) => self.evaluate_call_expr(expr),
            Expr::Index(expr) => self.evaluate_index_expr(expr),
            Expr::Access(expr) => self.evaluate_access_expr(expr),
        }
    }

    /// Evaluates the type of a literal expression.
    fn evaluate_literal_expr<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        expr: &LiteralExpr<N>,
    ) -> Option<Type> {
        match expr {
            LiteralExpr::Boolean(_) => Some(PrimitiveType::Boolean.into()),
            LiteralExpr::Integer(_) => Some(PrimitiveType::Integer.into()),
            LiteralExpr::Float(_) => Some(PrimitiveType::Float.into()),
            LiteralExpr::String(s) => {
                for p in s.parts() {
                    if let StringPart::Placeholder(p) = p {
                        self.check_placeholder(&p);
                    }
                }

                Some(PrimitiveType::String.into())
            }
            LiteralExpr::Array(expr) => Some(self.evaluate_literal_array(expr)),
            LiteralExpr::Pair(expr) => Some(self.evaluate_literal_pair(expr)),
            LiteralExpr::Map(expr) => Some(self.evaluate_literal_map(expr)),
            LiteralExpr::Object(expr) => Some(self.evaluate_literal_object(expr)),
            LiteralExpr::Struct(expr) => self.evaluate_literal_struct(expr),
            LiteralExpr::None(_) => Some(Type::None),
            LiteralExpr::Hints(expr) => self.evaluate_literal_hints(expr),
            LiteralExpr::Input(expr) => self.evaluate_literal_input(expr),
            LiteralExpr::Output(expr) => self.evaluate_literal_output(expr),
        }
    }

    /// Checks a placeholder expression.
    pub(crate) fn check_placeholder<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        placeholder: &Placeholder<N>,
    ) {
        self.placeholders += 1;

        // Evaluate the placeholder expression and check that the resulting type is
        // coercible to string for interpolation
        let expr = placeholder.expr();
        if let Some(ty) = self.evaluate_expr(&expr) {
            if let Some(option) = placeholder.option() {
                let valid = match option {
                    PlaceholderOption::Sep(_) => {
                        ty == Type::Union
                            || ty == Type::None
                            || matches!(&ty,
                        Type::Compound(CompoundType::Array(array_ty), _)
                        if matches!(array_ty.element_type(), Type::Primitive(_, false) | Type::Union))
                    }
                    PlaceholderOption::Default(_) => {
                        matches!(ty, Type::Primitive(..) | Type::Union | Type::None)
                    }
                    PlaceholderOption::TrueFalse(_) => {
                        matches!(
                            ty,
                            Type::Primitive(PrimitiveType::Boolean, _) | Type::Union | Type::None
                        )
                    }
                };

                if !valid {
                    self.context.add_diagnostic(invalid_placeholder_option(
                        &ty,
                        expr.span(),
                        &option,
                    ));
                }
            } else {
                match ty {
                    Type::Primitive(..) | Type::Union | Type::None => {}
                    _ => {
                        self.context
                            .add_diagnostic(cannot_coerce_to_string(&ty, expr.span()));
                    }
                }
            }
        }

        self.placeholders -= 1;
    }

    /// Evaluates the type of a literal array expression.
    fn evaluate_literal_array<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        expr: &LiteralArray<N>,
    ) -> Type {
        // Look at the first array element to determine the element type
        // The remaining elements must have a common type
        let mut elements = expr.elements();
        match elements
            .next()
            .and_then(|e| Some((self.evaluate_expr(&e)?, e.span())))
        {
            Some((mut expected, mut expected_span)) => {
                // Ensure the remaining element types share a common type
                for expr in elements {
                    if let Some(actual) = self.evaluate_expr(&expr) {
                        match expected.common_type(&actual) {
                            Some(ty) => {
                                expected = ty;
                                expected_span = expr.span();
                            }
                            _ => {
                                self.context.add_diagnostic(no_common_type(
                                    &expected,
                                    expected_span,
                                    &actual,
                                    expr.span(),
                                ));
                            }
                        }
                    }
                }

                ArrayType::new(expected).into()
            }
            // Treat empty array as `Array[Union]`
            None => ArrayType::new(Type::Union).into(),
        }
    }

    /// Evaluates the type of a literal pair expression.
    fn evaluate_literal_pair<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        expr: &LiteralPair<N>,
    ) -> Type {
        let (left, right) = expr.exprs();
        let left = self.evaluate_expr(&left).unwrap_or(Type::Union);
        let right = self.evaluate_expr(&right).unwrap_or(Type::Union);
        PairType::new(left, right).into()
    }

    /// Evaluates the type of a literal map expression.
    fn evaluate_literal_map<N: TreeNode + SyntaxNodeExt>(&mut self, expr: &LiteralMap<N>) -> Type {
        let map_item_type = |item: LiteralMapItem<N>| {
            let (key, value) = item.key_value();
            let expected_key = self.evaluate_expr(&key)?;
            match expected_key {
                Type::Primitive(..) | Type::None | Type::Union => {
                    // OK
                }
                _ => {
                    self.context
                        .add_diagnostic(map_key_not_primitive(key.span(), &expected_key));
                    return None;
                }
            }

            Some((
                expected_key,
                key.span(),
                self.evaluate_expr(&value)?,
                value.span(),
            ))
        };

        let mut items = expr.items();
        match items.next().and_then(map_item_type) {
            Some((
                mut expected_key,
                mut expected_key_span,
                mut expected_value,
                mut expected_value_span,
            )) => {
                // Ensure the remaining items types share common types
                for item in items {
                    let (key, value) = item.key_value();
                    if let Some(actual_key) = self.evaluate_expr(&key)
                        && let Some(actual_value) = self.evaluate_expr(&value)
                    {
                        match expected_key.common_type(&actual_key) {
                            Some(ty) => {
                                expected_key = ty;
                                expected_key_span = key.span();
                            }
                            _ => {
                                self.context.add_diagnostic(no_common_type(
                                    &expected_key,
                                    expected_key_span,
                                    &actual_key,
                                    key.span(),
                                ));
                            }
                        }

                        match expected_value.common_type(&actual_value) {
                            Some(ty) => {
                                expected_value = ty;
                                expected_value_span = value.span();
                            }
                            _ => {
                                self.context.add_diagnostic(no_common_type(
                                    &expected_value,
                                    expected_value_span,
                                    &actual_value,
                                    value.span(),
                                ));
                            }
                        }
                    }
                }

                MapType::new(expected_key, expected_value).into()
            }
            // Treat as `Map[Union, Union]`
            None => MapType::new(Type::Union, Type::Union).into(),
        }
    }

    /// Evaluates the type of a literal object expression.
    fn evaluate_literal_object<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        expr: &LiteralObject<N>,
    ) -> Type {
        // Validate the member expressions
        for item in expr.items() {
            let (_, v) = item.name_value();
            self.evaluate_expr(&v);
        }

        Type::Object
    }

    /// Evaluates the type of a literal struct expression.
    fn evaluate_literal_struct<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        expr: &LiteralStruct<N>,
    ) -> Option<Type> {
        let name = expr.name();
        match self.context.resolve_type_name(name.text(), name.span()) {
            Ok(ty) => {
                let ty = match ty {
                    Type::Compound(CompoundType::Struct(ty), false) => ty,
                    _ => panic!("type should be a required struct"),
                };

                // Keep track of which members are present in the expression
                let mut present = vec![false; ty.members().len()];

                // Validate the member types
                for item in expr.items() {
                    let (n, v) = item.name_value();
                    match ty.members.get_full(n.text()) {
                        Some((index, _, expected)) => {
                            present[index] = true;
                            if let Some(actual) = self.evaluate_expr(&v)
                                && !actual.is_coercible_to(expected)
                            {
                                self.context.add_diagnostic(type_mismatch(
                                    expected,
                                    n.span(),
                                    &actual,
                                    v.span(),
                                ));
                            }
                        }
                        _ => {
                            // Not a struct member
                            self.context
                                .add_diagnostic(not_a_struct_member(name.text(), &n));
                        }
                    }
                }

                // Find the first unspecified member that is required, if any
                let mut unspecified = present
                    .iter()
                    .enumerate()
                    .filter_map(|(i, present)| {
                        if *present {
                            return None;
                        }

                        let (name, ty) = &ty.members.get_index(i).unwrap();
                        if ty.is_optional() {
                            return None;
                        }

                        Some(name.as_str())
                    })
                    .peekable();

                if unspecified.peek().is_some() {
                    let mut members = String::new();
                    let mut count = 0;
                    while let Some(member) = unspecified.next() {
                        match (unspecified.peek().is_none(), count) {
                            (true, c) if c > 1 => members.push_str(", and "),
                            (true, 1) => members.push_str(" and "),
                            (false, c) if c > 0 => members.push_str(", "),
                            _ => {}
                        }

                        write!(&mut members, "`{member}`").ok();
                        count += 1;
                    }

                    self.context
                        .add_diagnostic(missing_struct_members(&name, count, &members));
                }

                Some(Type::Compound(CompoundType::Struct(ty), false))
            }
            Err(diagnostic) => {
                self.context.add_diagnostic(diagnostic);
                None
            }
        }
    }

    /// Evaluates a `runtime` section item.
    pub(crate) fn evaluate_runtime_item<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        name: &Ident<N::Token>,
        expr: &Expr<N>,
    ) {
        let expr_ty = self.evaluate_expr(expr).unwrap_or(Type::Union);
        if !self.evaluate_requirement(name, expr, &expr_ty) {
            // Always use object types for `runtime` section `inputs` and `outputs` keys as
            // only `hints` sections can use input/output hidden types
            if let Some(expected) = task_hint_types(self.context.version(), name.text(), false)
                && !expected
                    .iter()
                    .any(|target| expr_ty.is_coercible_to(target))
            {
                self.context.add_diagnostic(multiple_type_mismatch(
                    expected,
                    name.span(),
                    &expr_ty,
                    expr.span(),
                ));
            }
        }
    }

    /// Evaluates a `requirements` section item.
    pub(crate) fn evaluate_requirements_item<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        name: &Ident<N::Token>,
        expr: &Expr<N>,
    ) {
        let expr_ty = self.evaluate_expr(expr).unwrap_or(Type::Union);
        self.evaluate_requirement(name, expr, &expr_ty);
    }

    /// Evaluates a requirement in either a `requirements` section or a legacy
    /// `runtime` section.
    ///
    /// Returns `true` if the name matched a requirement or `false` if it did
    /// not.
    fn evaluate_requirement<N: TreeNode>(
        &mut self,
        name: &Ident<N::Token>,
        expr: &Expr<N>,
        expr_ty: &Type,
    ) -> bool {
        if let Some(expected) = task_requirement_types(self.context.version(), name.text()) {
            if !expected
                .iter()
                .any(|target| expr_ty.is_coercible_to(target))
            {
                self.context.add_diagnostic(multiple_type_mismatch(
                    expected,
                    name.span(),
                    expr_ty,
                    expr.span(),
                ));
            }

            return true;
        }

        false
    }

    /// Evaluates the type of a literal hints expression.
    fn evaluate_literal_hints<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        expr: &LiteralHints<N>,
    ) -> Option<Type> {
        self.context.task()?;

        for item in expr.items() {
            self.evaluate_hints_item(&item.name(), &item.expr())
        }

        Some(Type::Hidden(HiddenType::Hints))
    }

    /// Evaluates a hints item, whether in task `hints` section or a `hints`
    /// literal expression.
    pub(crate) fn evaluate_hints_item<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        name: &Ident<N::Token>,
        expr: &Expr<N>,
    ) {
        let expr_ty = self.evaluate_expr(expr).unwrap_or(Type::Union);
        if let Some(expected) = task_hint_types(self.context.version(), name.text(), true)
            && !expected
                .iter()
                .any(|target| expr_ty.is_coercible_to(target))
        {
            self.context.add_diagnostic(multiple_type_mismatch(
                expected,
                name.span(),
                &expr_ty,
                expr.span(),
            ));
        }
    }

    /// Evaluates the type of a literal input expression.
    fn evaluate_literal_input<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        expr: &LiteralInput<N>,
    ) -> Option<Type> {
        // Check to see if inputs literals are supported in the evaluation scope
        self.context.task()?;

        // Evaluate the items of the literal
        for item in expr.items() {
            self.evaluate_literal_io_item(item.names(), item.expr(), Io::Input);
        }

        Some(Type::Hidden(HiddenType::Input))
    }

    /// Evaluates the type of a literal output expression.
    fn evaluate_literal_output<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        expr: &LiteralOutput<N>,
    ) -> Option<Type> {
        // Check to see if output literals are supported in the evaluation scope
        self.context.task()?;

        // Evaluate the items of the literal
        for item in expr.items() {
            self.evaluate_literal_io_item(item.names(), item.expr(), Io::Output);
        }

        Some(Type::Hidden(HiddenType::Output))
    }

    /// Evaluates a literal input/output item.
    fn evaluate_literal_io_item<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        names: impl Iterator<Item = Ident<N::Token>>,
        expr: Expr<N>,
        io: Io,
    ) {
        let mut names = names.enumerate().peekable();
        let expr_ty = self.evaluate_expr(&expr).unwrap_or(Type::Union);

        // The first name should be an input/output and then the remainder should be a
        // struct member
        let mut span = None;
        let mut s: Option<&StructType> = None;
        while let Some((i, name)) = names.next() {
            // The first name is an input or an output
            let ty = if i == 0 {
                span = Some(name.span());

                match if io == Io::Input {
                    self.context
                        .task()
                        .expect("should have task")
                        .inputs()
                        .get(name.text())
                        .map(|i| i.ty())
                } else {
                    self.context
                        .task()
                        .expect("should have task")
                        .outputs()
                        .get(name.text())
                        .map(|o| o.ty())
                } {
                    Some(ty) => ty,
                    None => {
                        self.context.add_diagnostic(unknown_task_io(
                            self.context.task().expect("should have task").name(),
                            &name,
                            io,
                        ));
                        break;
                    }
                }
            } else {
                // Every other name is a struct member
                let start = span.unwrap().start();
                span = Some(Span::new(start, name.span().end() - start));
                let s = s.unwrap();
                match s.members.get(name.text()) {
                    Some(ty) => ty,
                    None => {
                        self.context
                            .add_diagnostic(not_a_struct_member(&s.name, &name));
                        break;
                    }
                }
            };

            match ty {
                Type::Compound(CompoundType::Struct(ty), _) => s = Some(ty),
                _ if names.peek().is_some() => {
                    self.context.add_diagnostic(not_a_struct(&name, i == 0));
                    break;
                }
                _ => {
                    // It's ok for the last one to not name a struct
                }
            }
        }

        // If we bailed out early above, calculate the entire span of the name
        if let Some((_, last)) = names.last() {
            let start = span.unwrap().start();
            span = Some(Span::new(start, last.span().end() - start));
        }

        // The type of every item should be `hints`
        if !expr_ty.is_coercible_to(&Type::Hidden(HiddenType::Hints)) {
            self.context.add_diagnostic(type_mismatch(
                &Type::Hidden(HiddenType::Hints),
                span.expect("should have span"),
                &expr_ty,
                expr.span(),
            ));
        }
    }

    /// Evaluates the type of an `if` expression.
    fn evaluate_if_expr<N: TreeNode + SyntaxNodeExt>(&mut self, expr: &IfExpr<N>) -> Option<Type> {
        let (cond_expr, true_expr, false_expr) = expr.exprs();

        // The conditional should be a boolean
        let cond_ty = self.evaluate_expr(&cond_expr).unwrap_or(Type::Union);
        if !cond_ty.is_coercible_to(&PrimitiveType::Boolean.into()) {
            self.context
                .add_diagnostic(if_conditional_mismatch(&cond_ty, cond_expr.span()));
        }

        // Check that the two expressions have the same type
        let true_ty = self.evaluate_expr(&true_expr).unwrap_or(Type::Union);
        let false_ty = self.evaluate_expr(&false_expr).unwrap_or(Type::Union);

        match (true_ty, false_ty) {
            (Type::Union, Type::Union) => None,
            (Type::Union, false_ty) => Some(false_ty),
            (true_ty, Type::Union) => Some(true_ty),
            (true_ty, false_ty) => match true_ty.common_type(&false_ty) {
                Some(ty) => Some(ty),
                _ => {
                    self.context.add_diagnostic(type_mismatch(
                        &true_ty,
                        true_expr.span(),
                        &false_ty,
                        false_expr.span(),
                    ));

                    None
                }
            },
        }
    }

    /// Evaluates the type of a `logical not` expression.
    fn evaluate_logical_not_expr<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        expr: &LogicalNotExpr<N>,
    ) -> Option<Type> {
        // The operand should be a boolean
        let operand = expr.operand();
        let ty = self.evaluate_expr(&operand).unwrap_or(Type::Union);
        if !ty.is_coercible_to(&PrimitiveType::Boolean.into()) {
            self.context
                .add_diagnostic(logical_not_mismatch(&ty, operand.span()));
        }

        Some(PrimitiveType::Boolean.into())
    }

    /// Evaluates the type of a negation expression.
    fn evaluate_negation_expr<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        expr: &NegationExpr<N>,
    ) -> Option<Type> {
        // The operand should be a int or float
        let operand = expr.operand();
        let ty = self.evaluate_expr(&operand)?;

        // If the type is `Int`, treat it as `Int`
        // This is checked first as `Int` is coercible to `Float`
        if ty.eq(&PrimitiveType::Integer.into()) {
            return Some(PrimitiveType::Integer.into());
        }

        if !ty.is_coercible_to(&PrimitiveType::Float.into()) {
            self.context
                .add_diagnostic(negation_mismatch(&ty, operand.span()));
            // Type is indeterminate as the expression may evaluate to more than one type
            return None;
        }

        Some(PrimitiveType::Float.into())
    }

    /// Evaluates the type of a `logical or` expression.
    fn evaluate_logical_or_expr<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        expr: &LogicalOrExpr<N>,
    ) -> Option<Type> {
        // Both operands should be booleans
        let (lhs, rhs) = expr.operands();

        let ty = self.evaluate_expr(&lhs).unwrap_or(Type::Union);
        if !ty.is_coercible_to(&PrimitiveType::Boolean.into()) {
            self.context
                .add_diagnostic(logical_or_mismatch(&ty, lhs.span()));
        }

        let ty = self.evaluate_expr(&rhs).unwrap_or(Type::Union);
        if !ty.is_coercible_to(&PrimitiveType::Boolean.into()) {
            self.context
                .add_diagnostic(logical_or_mismatch(&ty, rhs.span()));
        }

        Some(PrimitiveType::Boolean.into())
    }

    /// Evaluates the type of a `logical and` expression.
    fn evaluate_logical_and_expr<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        expr: &LogicalAndExpr<N>,
    ) -> Option<Type> {
        // Both operands should be booleans
        let (lhs, rhs) = expr.operands();

        let ty = self.evaluate_expr(&lhs).unwrap_or(Type::Union);
        if !ty.is_coercible_to(&PrimitiveType::Boolean.into()) {
            self.context
                .add_diagnostic(logical_and_mismatch(&ty, lhs.span()));
        }

        let ty = self.evaluate_expr(&rhs).unwrap_or(Type::Union);
        if !ty.is_coercible_to(&PrimitiveType::Boolean.into()) {
            self.context
                .add_diagnostic(logical_and_mismatch(&ty, rhs.span()));
        }

        Some(PrimitiveType::Boolean.into())
    }

    /// Evaluates the type of a comparison expression.
    fn evaluate_comparison_expr<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        op: ComparisonOperator,
        lhs: &Expr<N>,
        rhs: &Expr<N>,
        span: Span,
    ) -> Option<Type> {
        let lhs_ty = self.evaluate_expr(lhs).unwrap_or(Type::Union);
        let rhs_ty = self.evaluate_expr(rhs).unwrap_or(Type::Union);

        // Check for comparison to `None` or `Union` and allow it
        if lhs_ty.is_union() || lhs_ty.is_none() || rhs_ty.is_union() || rhs_ty.is_none() {
            return Some(PrimitiveType::Boolean.into());
        }

        // Check LHS and RHS for being coercible to one of the supported primitive types
        for expected in [
            Type::from(PrimitiveType::Boolean),
            PrimitiveType::Integer.into(),
            PrimitiveType::Float.into(),
            PrimitiveType::String.into(),
            PrimitiveType::File.into(),
            PrimitiveType::Directory.into(),
        ] {
            // Only support equality/inequality comparisons for `File` and `Directory`
            if op != ComparisonOperator::Equality
                && op != ComparisonOperator::Inequality
                && (matches!(
                    lhs_ty.as_primitive(),
                    Some(PrimitiveType::File) | Some(PrimitiveType::Directory)
                ) || matches!(
                    rhs_ty.as_primitive(),
                    Some(PrimitiveType::File) | Some(PrimitiveType::Directory)
                ))
            {
                continue;
            }

            if lhs_ty.is_coercible_to(&expected) && rhs_ty.is_coercible_to(&expected) {
                return Some(PrimitiveType::Boolean.into());
            }

            let expected = expected.optional();
            if lhs_ty.is_coercible_to(&expected) && rhs_ty.is_coercible_to(&expected) {
                return Some(PrimitiveType::Boolean.into());
            }
        }

        // For equality comparisons, check LHS and RHS being object and compound types
        if op == ComparisonOperator::Equality || op == ComparisonOperator::Inequality {
            // Check for object
            if (lhs_ty.is_coercible_to(&Type::Object) && rhs_ty.is_coercible_to(&Type::Object))
                || (lhs_ty.is_coercible_to(&Type::OptionalObject)
                    && rhs_ty.is_coercible_to(&Type::OptionalObject))
            {
                return Some(PrimitiveType::Boolean.into());
            }

            // Check for other compound types
            let equal = match (&lhs_ty, &rhs_ty) {
                (
                    Type::Compound(CompoundType::Array(a), _),
                    Type::Compound(CompoundType::Array(b), _),
                ) => a == b,
                (
                    Type::Compound(CompoundType::Pair(a), _),
                    Type::Compound(CompoundType::Pair(b), _),
                ) => a == b,
                (
                    Type::Compound(CompoundType::Map(a), _),
                    Type::Compound(CompoundType::Map(b), _),
                ) => a == b,
                (
                    Type::Compound(CompoundType::Struct(a), _),
                    Type::Compound(CompoundType::Struct(b), _),
                ) => a == b,
                _ => false,
            };

            if equal {
                return Some(PrimitiveType::Boolean.into());
            }
        }

        // A type mismatch at this point
        self.context.add_diagnostic(comparison_mismatch(
            op,
            span,
            &lhs_ty,
            lhs.span(),
            &rhs_ty,
            rhs.span(),
        ));
        Some(PrimitiveType::Boolean.into())
    }

    /// Evaluates the type of a numeric expression.
    fn evaluate_numeric_expr<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        op: NumericOperator,
        span: Span,
        lhs: &Expr<N>,
        rhs: &Expr<N>,
    ) -> Option<Type> {
        let lhs_ty = self.evaluate_expr(lhs).unwrap_or(Type::Union);
        let rhs_ty = self.evaluate_expr(rhs).unwrap_or(Type::Union);

        // If both sides are `Int`, the result is `Int`
        if lhs_ty.eq(&PrimitiveType::Integer.into()) && rhs_ty.eq(&PrimitiveType::Integer.into()) {
            return Some(PrimitiveType::Integer.into());
        }

        // If both sides are coercible to `Float`, the result is `Float`
        if !lhs_ty.is_union()
            && lhs_ty.is_coercible_to(&PrimitiveType::Float.into())
            && !rhs_ty.is_union()
            && rhs_ty.is_coercible_to(&PrimitiveType::Float.into())
        {
            return Some(PrimitiveType::Float.into());
        }

        // For addition, also support `String` on one or both sides of any primitive
        // type that isn't `Boolean`; in placeholder expressions, allow the
        // other side to also be optional
        if op == NumericOperator::Addition {
            let allow_optional = self.placeholders > 0;
            let other = if (!lhs_ty.is_optional() || allow_optional)
                && lhs_ty
                    .as_primitive()
                    .map(|p| p == PrimitiveType::String)
                    .unwrap_or(false)
            {
                Some((lhs_ty.is_optional(), &rhs_ty, rhs.span()))
            } else if (!rhs_ty.is_optional() || allow_optional)
                && rhs_ty
                    .as_primitive()
                    .map(|p| p == PrimitiveType::String)
                    .unwrap_or(false)
            {
                Some((rhs_ty.is_optional(), &lhs_ty, lhs.span()))
            } else {
                None
            };

            if let Some((optional, other, span)) = other {
                if (!other.is_optional() || allow_optional)
                    && other
                        .as_primitive()
                        .map(|p| p != PrimitiveType::Boolean)
                        .unwrap_or(other.is_union() || (allow_optional && other.is_none()))
                {
                    let ty: Type = PrimitiveType::String.into();
                    if optional || other.is_optional() {
                        return Some(ty.optional());
                    }

                    return Some(ty);
                }

                self.context
                    .add_diagnostic(string_concat_mismatch(other, span));
                return None;
            }
        }

        if !lhs_ty.is_union() && !rhs_ty.is_union() {
            self.context.add_diagnostic(numeric_mismatch(
                op,
                span,
                &lhs_ty,
                lhs.span(),
                &rhs_ty,
                rhs.span(),
            ));
        }

        None
    }

    /// Evaluates the type of a call expression.
    fn evaluate_call_expr<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        expr: &CallExpr<N>,
    ) -> Option<Type> {
        let target = expr.target();
        match STDLIB.function(target.text()) {
            Some(f) => {
                // Evaluate the argument expressions
                let mut count = 0;
                let mut arguments = [const { Type::Union }; MAX_PARAMETERS];

                for arg in expr.arguments() {
                    if count < MAX_PARAMETERS {
                        arguments[count] = self.evaluate_expr(&arg).unwrap_or(Type::Union);
                    }

                    count += 1;
                }

                match target.text() {
                    "find" | "matches" | "sub" => {
                        // above function expect the pattern as 2nd argument
                        if let Some(Expr::Literal(LiteralExpr::String(pattern_literal))) =
                            expr.arguments().nth(1)
                            && let Some(value) = pattern_literal.text()
                        {
                            let pattern = value.text().to_string();
                            if let Err(e) = regex::Regex::new(&pattern) {
                                self.context.add_diagnostic(invalid_regex_pattern(
                                    target.text(),
                                    value.text(),
                                    &e,
                                    pattern_literal.span(),
                                ));
                            }
                        }
                    }
                    _ => {}
                }

                let arguments = &arguments[..count.min(MAX_PARAMETERS)];
                if count <= MAX_PARAMETERS {
                    match f.bind(self.context.version(), arguments) {
                        Ok(binding) => {
                            if let Some(severity) =
                                self.context.diagnostics_config().unnecessary_function_call
                                && !expr.inner().is_rule_excepted(UNNECESSARY_FUNCTION_CALL)
                            {
                                self.check_unnecessary_call(
                                    &target,
                                    arguments,
                                    expr.arguments().map(|e| e.span()),
                                    severity,
                                );
                            }
                            return Some(binding.return_type().clone());
                        }
                        Err(FunctionBindError::RequiresVersion(minimum)) => {
                            self.context.add_diagnostic(unsupported_function(
                                minimum,
                                target.text(),
                                target.span(),
                            ));
                        }
                        Err(FunctionBindError::TooFewArguments(minimum)) => {
                            self.context.add_diagnostic(too_few_arguments(
                                target.text(),
                                target.span(),
                                minimum,
                                count,
                            ));
                        }
                        Err(FunctionBindError::TooManyArguments(maximum)) => {
                            self.context.add_diagnostic(too_many_arguments(
                                target.text(),
                                target.span(),
                                maximum,
                                count,
                                expr.arguments().skip(maximum).map(|e| e.span()),
                            ));
                        }
                        Err(FunctionBindError::ArgumentTypeMismatch { index, expected }) => {
                            self.context.add_diagnostic(argument_type_mismatch(
                                target.text(),
                                &expected,
                                &arguments[index],
                                expr.arguments()
                                    .nth(index)
                                    .map(|e| e.span())
                                    .expect("should have span"),
                            ));
                        }
                        Err(FunctionBindError::Ambiguous { first, second }) => {
                            self.context.add_diagnostic(ambiguous_argument(
                                target.text(),
                                target.span(),
                                &first,
                                &second,
                            ));
                        }
                    }
                } else {
                    // Exceeded the maximum number of arguments to any function
                    match f.param_min_max(self.context.version()) {
                        Some((_, max)) => {
                            assert!(max <= MAX_PARAMETERS);
                            self.context.add_diagnostic(too_many_arguments(
                                target.text(),
                                target.span(),
                                max,
                                count,
                                expr.arguments().skip(max).map(|e| e.span()),
                            ));
                        }
                        None => {
                            self.context.add_diagnostic(unsupported_function(
                                f.minimum_version(),
                                target.text(),
                                target.span(),
                            ));
                        }
                    }
                }

                Some(f.realize_unconstrained_return_type(arguments))
            }
            None => {
                self.context
                    .add_diagnostic(unknown_function(target.text(), target.span()));
                None
            }
        }
    }

    /// Evaluates the type of an index expression.
    fn evaluate_index_expr<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        expr: &IndexExpr<N>,
    ) -> Option<Type> {
        let (target, index) = expr.operands();

        // Determine the expected index type and result type of the expression
        let target_ty = self.evaluate_expr(&target)?;
        let (expected_index_ty, result_ty) = match &target_ty {
            Type::Compound(CompoundType::Array(ty), _) => (
                Some(PrimitiveType::Integer.into()),
                Some(ty.element_type().clone()),
            ),
            Type::Compound(CompoundType::Map(ty), _) => {
                (Some(ty.key_type().clone()), Some(ty.value_type().clone()))
            }
            _ => (None, None),
        };

        // Check that the index type is the expected one
        if let Some(expected_index_ty) = expected_index_ty {
            let index_ty = self.evaluate_expr(&index).unwrap_or(Type::Union);
            if !index_ty.is_coercible_to(&expected_index_ty) {
                self.context.add_diagnostic(index_type_mismatch(
                    &expected_index_ty,
                    &index_ty,
                    index.span(),
                ));
            }
        }

        match result_ty {
            Some(ty) => Some(ty),
            None => {
                self.context
                    .add_diagnostic(cannot_index(&target_ty, target.span()));
                None
            }
        }
    }

    /// Evaluates the type of an access expression.
    fn evaluate_access_expr<N: TreeNode + SyntaxNodeExt>(
        &mut self,
        expr: &AccessExpr<N>,
    ) -> Option<Type> {
        let (target, name) = expr.operands();
        let ty = self.evaluate_expr(&target)?;

        match &ty {
            Type::Hidden(HiddenType::TaskPreEvaluation) => {
                return match task_member_type_pre_evaluation(name.text()) {
                    Some(ty) => Some(ty),
                    None => {
                        self.context.add_diagnostic(not_a_task_member(&name));
                        return None;
                    }
                };
            }
            Type::Hidden(HiddenType::TaskPostEvaluation) => {
                return match task_member_type_post_evaluation(
                    self.context.version(),
                    name.text(),
                ) {
                    Some(ty) => Some(ty),
                    None => {
                        self.context.add_diagnostic(not_a_task_member(&name));
                        return None;
                    }
                };
            }
            Type::Hidden(HiddenType::PreviousRequirements) => {
                return match previous_requirements_member_type(name.text()) {
                    Some(ty) => Some(ty),
                    None => {
                        self.context.add_diagnostic(not_a_previous_requirements_member(&name));
                        return None;
                    }
                };
            }
            _ => {}
        }

        // Check to see if it's a compound type or call output
        match &ty {
            Type::Compound(CompoundType::Struct(ty), _) => {
                if let Some(ty) = ty.members.get(name.text()) {
                    return Some(ty.clone());
                }

                self.context
                    .add_diagnostic(not_a_struct_member(ty.name(), &name));
                return None;
            }
            Type::Compound(CompoundType::Pair(ty), _) => {
                // Support `left` and `right` accessors for pairs
                return match name.text() {
                    "left" => Some(ty.left_type.clone()),
                    "right" => Some(ty.right_type.clone()),
                    _ => {
                        self.context.add_diagnostic(not_a_pair_accessor(&name));
                        None
                    }
                };
            }
            Type::Call(ty) => {
                if let Some(output) = ty.outputs().get(name.text()) {
                    return Some(output.ty().clone());
                }

                self.context
                    .add_diagnostic(unknown_call_io(ty, &name, Io::Output));
                return None;
            }
            _ => {}
        }

        // Check to see if it's coercible to object; if so, treat as `Union` as it's
        // indeterminate
        if ty.is_coercible_to(&Type::OptionalObject) {
            return Some(Type::Union);
        }

        self.context
            .add_diagnostic(cannot_access(&ty, target.span()));
        None
    }

    /// Checks for unnecessary function calls.
    fn check_unnecessary_call<T: TreeToken>(
        &mut self,
        target: &Ident<T>,
        arguments: &[Type],
        mut spans: impl Iterator<Item = Span>,
        severity: Severity,
    ) {
        let (label, span, fix) = match target.text() {
            "select_first" => {
                if let Some(ty) = arguments[0].as_array().map(|a| a.element_type()) {
                    if ty.is_optional() || ty.is_union() {
                        return;
                    }
                    (
                        format!("array element type `{ty}` is not optional"),
                        spans.next().expect("should have span"),
                        "replace the function call with the array's first element",
                    )
                } else {
                    return;
                }
            }
            "select_all" => {
                if let Some(ty) = arguments[0].as_array().map(|a| a.element_type()) {
                    if ty.is_optional() || ty.is_union() {
                        return;
                    }
                    (
                        format!("array element type `{ty}` is not optional"),
                        spans.next().expect("should have span"),
                        "replace the function call with the array itself",
                    )
                } else {
                    return;
                }
            }
            "defined" => {
                if arguments[0].is_optional() || arguments[0].is_union() {
                    return;
                }

                (
                    format!("type `{ty}` is not optional", ty = arguments[0]),
                    spans.next().expect("should have span"),
                    "replace the function call with `true`",
                )
            }
            _ => return,
        };

        self.context.add_diagnostic(
            unnecessary_function_call(target.text(), target.span(), &label, span)
                .with_severity(severity)
                .with_fix(fix),
        )
    }
}

//! Type conversion helpers for a V1 AST.

use std::fmt;
use std::fmt::Write;
use std::sync::Arc;
use std::sync::LazyLock;

use wdl_ast::AstNodeExt;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
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
use wdl_ast::version::V1;

use super::ArrayType;
use super::CompoundTypeDef;
use super::MapType;
use super::Optional;
use super::PairType;
use super::PrimitiveType;
use super::PrimitiveTypeKind;
use super::StructType;
use super::Type;
use super::TypeEq;
use super::Types;
use crate::diagnostics::Io;
use crate::diagnostics::ambiguous_argument;
use crate::diagnostics::argument_type_mismatch;
use crate::diagnostics::cannot_access;
use crate::diagnostics::cannot_coerce_to_string;
use crate::diagnostics::cannot_index;
use crate::diagnostics::comparison_mismatch;
use crate::diagnostics::if_conditional_mismatch;
use crate::diagnostics::index_type_mismatch;
use crate::diagnostics::logical_and_mismatch;
use crate::diagnostics::logical_not_mismatch;
use crate::diagnostics::logical_or_mismatch;
use crate::diagnostics::map_key_not_primitive;
use crate::diagnostics::missing_struct_members;
use crate::diagnostics::negation_mismatch;
use crate::diagnostics::no_common_type;
use crate::diagnostics::not_a_pair_accessor;
use crate::diagnostics::not_a_struct;
use crate::diagnostics::not_a_struct_member;
use crate::diagnostics::not_a_task_member;
use crate::diagnostics::numeric_mismatch;
use crate::diagnostics::string_concat_mismatch;
use crate::diagnostics::too_few_arguments;
use crate::diagnostics::too_many_arguments;
use crate::diagnostics::type_mismatch;
use crate::diagnostics::type_mismatch_custom;
use crate::diagnostics::unknown_call_io;
use crate::diagnostics::unknown_function;
use crate::diagnostics::unknown_task_io;
use crate::diagnostics::unsupported_function;
use crate::document::Input;
use crate::document::Output;
use crate::stdlib::FunctionBindError;
use crate::stdlib::STDLIB;
use crate::types::Coercible;

/// Gets the type of a `task` variable member type.
///
/// `task` variables are supported in command and output sections in WDL 1.2.
///
/// Returns `None` if the given member name is unknown.
pub fn task_member_type(name: &str) -> Option<Type> {
    match name {
        "name" | "id" | "container" => Some(PrimitiveTypeKind::String.into()),
        "cpu" => Some(PrimitiveTypeKind::Float.into()),
        "memory" | "attempt" => Some(PrimitiveTypeKind::Integer.into()),
        "gpu" | "fpga" => Some(STDLIB.array_string),
        "disks" => Some(STDLIB.map_string_int),
        "end_time" | "return_code" => Some(Type::from(PrimitiveTypeKind::Integer).optional()),
        "meta" | "parameter_meta" | "ext" => Some(Type::Object),
        _ => None,
    }
}

/// Gets the types of a task requirement.
///
/// Returns a slice of types or `None` if the given name is not a requirement.
pub fn task_requirement_types(version: SupportedVersion, name: &str) -> Option<&'static [Type]> {
    /// The types for the `container` requirement.
    static CONTAINER_TYPES: LazyLock<Box<[Type]>> =
        LazyLock::new(|| Box::new([PrimitiveTypeKind::String.into(), STDLIB.array_string]));
    /// The types for the `cpu` requirement.
    const CPU_TYPES: &[Type] = &[
        Type::Primitive(PrimitiveType::new(PrimitiveTypeKind::Integer)),
        Type::Primitive(PrimitiveType::new(PrimitiveTypeKind::Float)),
    ];
    /// The types for the `memory` requirement.
    const MEMORY_TYPES: &[Type] = &[
        Type::Primitive(PrimitiveType::new(PrimitiveTypeKind::Integer)),
        Type::Primitive(PrimitiveType::new(PrimitiveTypeKind::String)),
    ];
    /// The types for the `gpu` requirement.
    const GPU_TYPES: &[Type] = &[Type::Primitive(PrimitiveType::new(
        PrimitiveTypeKind::Boolean,
    ))];
    /// The types for the `fpga` requirement.
    const FPGA_TYPES: &[Type] = &[Type::Primitive(PrimitiveType::new(
        PrimitiveTypeKind::Boolean,
    ))];
    /// The types for the `disks` requirement.
    static DISKS_TYPES: LazyLock<Box<[Type]>> = LazyLock::new(|| {
        Box::new([
            PrimitiveTypeKind::Integer.into(),
            PrimitiveTypeKind::String.into(),
            STDLIB.array_string,
        ])
    });
    /// The types for the `max_retries` requirement.
    const MAX_RETRIES_TYPES: &[Type] = &[Type::Primitive(PrimitiveType::new(
        PrimitiveTypeKind::Integer,
    ))];
    /// The types for the `return_codes` requirement.
    static RETURN_CODES_TYPES: LazyLock<Box<[Type]>> = LazyLock::new(|| {
        Box::new([
            PrimitiveTypeKind::Integer.into(),
            PrimitiveTypeKind::String.into(),
            STDLIB.array_int,
        ])
    });

    match name {
        "container" | "docker" => Some(&CONTAINER_TYPES),
        "cpu" => Some(CPU_TYPES),
        "disks" => Some(&DISKS_TYPES),
        "gpu" => Some(GPU_TYPES),
        "fpga" if version >= SupportedVersion::V1(V1::Two) => Some(FPGA_TYPES),
        "max_retries" if version >= SupportedVersion::V1(V1::Two) => Some(MAX_RETRIES_TYPES),
        "maxRetries" => Some(MAX_RETRIES_TYPES),
        "memory" => Some(MEMORY_TYPES),
        "return_codes" if version >= SupportedVersion::V1(V1::Two) => Some(&RETURN_CODES_TYPES),
        "returnCodes" => Some(&RETURN_CODES_TYPES),
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
    static DISKS_TYPES: LazyLock<Box<[Type]>> =
        LazyLock::new(|| Box::new([PrimitiveTypeKind::String.into(), STDLIB.map_string_string]));
    /// The types for the `fpga` hint.
    const FPGA_TYPES: &[Type] = &[
        Type::Primitive(PrimitiveType::new(PrimitiveTypeKind::Integer)),
        Type::Primitive(PrimitiveType::new(PrimitiveTypeKind::String)),
    ];
    /// The types for the `gpu` hint.
    const GPU_TYPES: &[Type] = &[
        Type::Primitive(PrimitiveType::new(PrimitiveTypeKind::Integer)),
        Type::Primitive(PrimitiveType::new(PrimitiveTypeKind::String)),
    ];
    /// The types for the `inputs` hint.
    const INPUTS_TYPES: &[Type] = &[Type::Object];
    /// The types for the `inputs` hint (with hidden types).
    const INPUTS_HIDDEN_TYPES: &[Type] = &[Type::Input];
    /// The types for the `localization_optional` hint.
    const LOCALIZATION_OPTIONAL_TYPES: &[Type] = &[Type::Primitive(PrimitiveType::new(
        PrimitiveTypeKind::Boolean,
    ))];
    /// The types for the `max_cpu` hint.
    const MAX_CPU_TYPES: &[Type] = &[
        Type::Primitive(PrimitiveType::new(PrimitiveTypeKind::Integer)),
        Type::Primitive(PrimitiveType::new(PrimitiveTypeKind::Float)),
    ];
    /// The types for the `max_memory` hint.
    const MAX_MEMORY_TYPES: &[Type] = &[
        Type::Primitive(PrimitiveType::new(PrimitiveTypeKind::Integer)),
        Type::Primitive(PrimitiveType::new(PrimitiveTypeKind::String)),
    ];
    /// The types for the `outputs` hint.
    const OUTPUTS_TYPES: &[Type] = &[Type::Object];
    /// The types for the `outputs` hint (with hidden types).
    const OUTPUTS_HIDDEN_TYPES: &[Type] = &[Type::Output];
    /// The types for the `short_task` hint.
    const SHORT_TASK_TYPES: &[Type] = &[Type::Primitive(PrimitiveType::new(
        PrimitiveTypeKind::Boolean,
    ))];

    match name {
        "disks" => Some(&DISKS_TYPES),
        "fpga" if version >= SupportedVersion::V1(V1::Two) => Some(FPGA_TYPES),
        "gpu" => Some(GPU_TYPES),
        "inputs" if use_hidden_types && version >= SupportedVersion::V1(V1::Two) => {
            Some(INPUTS_HIDDEN_TYPES)
        }
        "inputs" => Some(INPUTS_TYPES),
        "localization_optional" if version >= SupportedVersion::V1(V1::Two) => {
            Some(LOCALIZATION_OPTIONAL_TYPES)
        }
        "localizationOptional" => Some(LOCALIZATION_OPTIONAL_TYPES),
        "max_cpu" if version >= SupportedVersion::V1(V1::Two) => Some(MAX_CPU_TYPES),
        "maxCpu" => Some(MAX_CPU_TYPES),
        "max_memory" if version >= SupportedVersion::V1(V1::Two) => Some(MAX_MEMORY_TYPES),
        "maxMemory" => Some(MAX_MEMORY_TYPES),
        "outputs" if use_hidden_types && version >= SupportedVersion::V1(V1::Two) => {
            Some(OUTPUTS_HIDDEN_TYPES)
        }
        "outputs" => Some(OUTPUTS_TYPES),
        "short_task" if version >= SupportedVersion::V1(V1::Two) => Some(SHORT_TASK_TYPES),
        "shortTask" => Some(SHORT_TASK_TYPES),
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
        write!(f, "{}", match self {
            Self::Equality => "==",
            Self::Inequality => "!=",
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
        })
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
        write!(f, "{}", match self {
            Self::Addition => "addition",
            Self::Subtraction => "subtraction",
            Self::Multiplication => "multiplication",
            Self::Division => "division",
            Self::Modulo => "remainder",
            Self::Exponentiation => "exponentiation",
        })
    }
}

/// Used to convert AST types into diagnostic types.
#[derive(Debug)]
pub struct AstTypeConverter<'a, L> {
    /// The types collection to use for the conversion.
    types: &'a mut Types,
    /// A lookup function for looking up type names.
    lookup: L,
}

impl<'a, L> AstTypeConverter<'a, L>
where
    L: FnMut(&str, Span) -> Result<Type, Diagnostic>,
{
    /// Constructs a new AST type converter.
    ///
    /// The provided callback is used to look up type name references.
    pub fn new(types: &'a mut Types, lookup: L) -> Self {
        Self { types, lookup }
    }

    /// Converts a V1 AST type into an analysis type.
    ///
    /// If a type could not created, an error with the relevant diagnostic is
    /// returned.
    pub fn convert_type(&mut self, ty: &v1::Type) -> Result<Type, Diagnostic> {
        let optional = ty.is_optional();

        let ty = match ty {
            v1::Type::Map(ty) => {
                let ty = self.convert_map_type(ty)?;
                self.types.add_map(ty)
            }
            v1::Type::Array(ty) => {
                let ty = self.convert_array_type(ty)?;
                self.types.add_array(ty)
            }
            v1::Type::Pair(ty) => {
                let ty = self.convert_pair_type(ty)?;
                self.types.add_pair(ty)
            }
            v1::Type::Object(_) => Type::Object,
            v1::Type::Ref(r) => {
                let name = r.name();
                (self.lookup)(name.as_str(), name.span())?
            }
            v1::Type::Primitive(ty) => Type::Primitive(ty.into()),
        };

        if optional { Ok(ty.optional()) } else { Ok(ty) }
    }

    /// Converts an AST array type to a diagnostic array type.
    ///
    /// If a type could not created, an error with the relevant diagnostic is
    /// returned.
    pub fn convert_array_type(&mut self, ty: &v1::ArrayType) -> Result<ArrayType, Diagnostic> {
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
    pub fn convert_pair_type(&mut self, ty: &v1::PairType) -> Result<PairType, Diagnostic> {
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
    pub fn convert_map_type(&mut self, ty: &v1::MapType) -> Result<MapType, Diagnostic> {
        let (key_type, value_type) = ty.types();
        Ok(MapType::new(
            Type::Primitive((&key_type).into()),
            self.convert_type(&value_type)?,
        ))
    }

    /// Converts an AST struct definition into a struct type.
    ///
    /// If the type could not created, an error with the relevant diagnostic is
    /// returned.
    pub fn convert_struct_type(
        &mut self,
        definition: &v1::StructDefinition,
    ) -> Result<StructType, Diagnostic> {
        Ok(StructType {
            name: Arc::new(definition.name().as_str().into()),
            members: definition
                .members()
                .map(|d| Ok((d.name().as_str().to_string(), self.convert_type(&d.ty())?)))
                .collect::<Result<_, _>>()?,
        })
    }
}

impl From<v1::PrimitiveTypeKind> for PrimitiveTypeKind {
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

impl From<&v1::PrimitiveType> for PrimitiveType {
    fn from(ty: &v1::PrimitiveType) -> Self {
        let kind = ty.kind().into();
        if ty.is_optional() {
            Self::optional(kind)
        } else {
            Self::new(kind)
        }
    }
}

/// Represents context to an expression type evaluator.
pub trait EvaluationContext {
    /// Gets the supported version of the document being evaluated.
    fn version(&self) -> SupportedVersion;

    /// Gets the types collection associated with the evaluation.
    fn types(&self) -> &Types;

    /// Gets the mutable types collection associated with the evaluation.
    fn types_mut(&mut self) -> &mut Types;

    /// Gets the type of the given name in scope.
    fn resolve_name(&self, name: &Ident) -> Option<Type>;

    /// Resolves a type name to a type.
    fn resolve_type_name(&mut self, name: &Ident) -> Result<Type, Diagnostic>;

    /// Gets an input for the given name.
    ///
    /// Returns `None` if `input` hidden types are not supported or if the
    /// specified input isn't known.
    fn input(&self, name: &str) -> Option<Input>;

    /// Gets an output for the given name.
    ///
    /// Returns `None` if `output` hidden types are not supported or if the
    /// specified output isn't known.
    fn output(&self, name: &str) -> Option<Output>;

    /// The task name associated with the evaluation.
    ///
    /// Returns `None` if no task is visible in this context.
    fn task_name(&self) -> Option<&str>;

    /// Whether or not `hints` hidden types are supported for the evaluation.
    fn supports_hints_type(&self) -> bool;

    /// Whether or not `input` hidden types are supported for the evaluation.
    fn supports_input_type(&self) -> bool;

    /// Whether or not `output` hidden types are supported for the evaluation.
    fn supports_output_type(&self) -> bool;
}

/// Represents an evaluator of expression types.
#[derive(Debug)]
pub struct ExprTypeEvaluator<'a, C> {
    /// The context for the evaluator.
    context: &'a mut C,
    /// The diagnostics collection for adding evaluation diagnostics.
    diagnostics: &'a mut Vec<Diagnostic>,
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
    pub fn new(context: &'a mut C, diagnostics: &'a mut Vec<Diagnostic>) -> Self {
        Self {
            context,
            diagnostics,
            placeholders: 0,
        }
    }

    /// Evaluates the type of the given expression in the given scope.
    ///
    /// Returns `None` if the type of the expression is indeterminate.
    pub fn evaluate_expr(&mut self, expr: &Expr) -> Option<Type> {
        match expr {
            Expr::Literal(expr) => self.evaluate_literal_expr(expr),
            Expr::Name(r) => self.context.resolve_name(&r.name()),
            Expr::Parenthesized(expr) => self.evaluate_expr(&expr.inner()),
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
    fn evaluate_literal_expr(&mut self, expr: &LiteralExpr) -> Option<Type> {
        match expr {
            LiteralExpr::Boolean(_) => Some(PrimitiveTypeKind::Boolean.into()),
            LiteralExpr::Integer(_) => Some(PrimitiveTypeKind::Integer.into()),
            LiteralExpr::Float(_) => Some(PrimitiveTypeKind::Float.into()),
            LiteralExpr::String(s) => {
                for p in s.parts() {
                    if let StringPart::Placeholder(p) = p {
                        self.check_placeholder(&p);
                    }
                }

                Some(PrimitiveTypeKind::String.into())
            }
            LiteralExpr::Array(expr) => Some(self.evaluate_literal_array(expr)),
            LiteralExpr::Pair(expr) => Some(self.evaluate_literal_pair(expr)),
            LiteralExpr::Map(expr) => Some(self.evaluate_literal_map(expr)),
            LiteralExpr::Object(_) => Some(Type::Object),
            LiteralExpr::Struct(expr) => self.evaluate_literal_struct(expr),
            LiteralExpr::None(_) => Some(Type::None),
            LiteralExpr::Hints(expr) => self.evaluate_literal_hints(expr),
            LiteralExpr::Input(expr) => self.evaluate_literal_input(expr),
            LiteralExpr::Output(expr) => self.evaluate_literal_output(expr),
        }
    }

    /// Checks a placeholder expression.
    pub(crate) fn check_placeholder(&mut self, placeholder: &Placeholder) {
        self.placeholders += 1;

        // Evaluate the placeholder expression and check that the resulting type is
        // coercible to string for interpolation
        let expr = placeholder.expr();
        if let Some(ty) = self.evaluate_expr(&expr) {
            match ty {
                Type::Primitive(_) | Type::Union | Type::None => {
                    // OK
                }
                _ => {
                    // Check for a sep option is specified; if so, accept `Array[P]` where `P` is
                    // primitive.
                    let mut coercible = false;
                    if let Some(PlaceholderOption::Sep(_)) = placeholder.option() {
                        if let Type::Compound(c) = ty {
                            if let CompoundTypeDef::Array(a) =
                                self.context.types().type_definition(c.definition())
                            {
                                if !a.element_type().is_optional()
                                    && a.element_type().as_primitive().is_some()
                                {
                                    // OK
                                    coercible = true;
                                }
                            }
                        }
                    }

                    if !coercible {
                        self.diagnostics.push(cannot_coerce_to_string(
                            self.context.types(),
                            ty,
                            expr.span(),
                        ));
                    }
                }
            }
        }

        self.placeholders -= 1;
    }

    /// Evaluates the type of a literal array expression.
    fn evaluate_literal_array(&mut self, expr: &LiteralArray) -> Type {
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
                        if let Some(ty) = actual.common_type(self.context.types(), expected) {
                            expected = ty;
                            expected_span = expr.span();
                        } else {
                            self.diagnostics.push(no_common_type(
                                self.context.types(),
                                expected,
                                expected_span,
                                actual,
                                expr.span(),
                            ));
                        }
                    }
                }

                self.context
                    .types_mut()
                    .add_array(ArrayType::non_empty(expected))
            }
            // Treat empty array as `Array[Union]`
            None => self
                .context
                .types_mut()
                .add_array(ArrayType::new(Type::Union)),
        }
    }

    /// Evaluates the type of a literal pair expression.
    fn evaluate_literal_pair(&mut self, expr: &LiteralPair) -> Type {
        let (left, right) = expr.exprs();
        let left = self.evaluate_expr(&left).unwrap_or(Type::Union);
        let right = self.evaluate_expr(&right).unwrap_or(Type::Union);
        self.context
            .types_mut()
            .add_pair(PairType::new(left, right))
    }

    /// Evaluates the type of a literal map expression.
    fn evaluate_literal_map(&mut self, expr: &LiteralMap) -> Type {
        let map_item_type = |item: LiteralMapItem| {
            let (key, value) = item.key_value();
            let expected_key = self.evaluate_expr(&key)?;
            match expected_key {
                Type::Primitive(_) | Type::None | Type::Union => {
                    // OK
                }
                _ => {
                    self.diagnostics.push(map_key_not_primitive(
                        self.context.types(),
                        key.span(),
                        expected_key,
                    ));
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
                    if let Some(actual_key) = self.evaluate_expr(&key) {
                        if let Some(actual_value) = self.evaluate_expr(&value) {
                            if let Some(ty) =
                                actual_key.common_type(self.context.types(), expected_key)
                            {
                                expected_key = ty;
                                expected_key_span = key.span();
                            } else {
                                self.diagnostics.push(no_common_type(
                                    self.context.types(),
                                    expected_key,
                                    expected_key_span,
                                    actual_key,
                                    key.span(),
                                ));
                            }

                            if let Some(ty) =
                                actual_value.common_type(self.context.types(), expected_value)
                            {
                                expected_value = ty;
                                expected_value_span = value.span();
                            } else {
                                self.diagnostics.push(no_common_type(
                                    self.context.types(),
                                    expected_value,
                                    expected_value_span,
                                    actual_value,
                                    value.span(),
                                ));
                            }
                        }
                    }
                }

                self.context
                    .types_mut()
                    .add_map(MapType::new(expected_key, expected_value))
            }
            // Treat as `Map[Union, Union]`
            None => self
                .context
                .types_mut()
                .add_map(MapType::new(Type::Union, Type::Union)),
        }
    }

    /// Evaluates the type of a literal struct expression.
    fn evaluate_literal_struct(&mut self, expr: &LiteralStruct) -> Option<Type> {
        let name = expr.name();
        match self.context.resolve_type_name(&name) {
            Ok(ty) => {
                let id = match ty {
                    Type::Compound(ty) => ty.definition(),
                    _ => panic!("type should be compound"),
                };

                // Keep track of which members are present in the expression
                let mut present = vec![
                    false;
                    self.context
                        .types()
                        .type_definition(id)
                        .as_struct()
                        .expect("should be a struct")
                        .members()
                        .len()
                ];

                // Validate the member types
                for item in expr.items() {
                    let (n, v) = item.name_value();
                    if let Some((index, _, expected)) = self
                        .context
                        .types()
                        .type_definition(id)
                        .as_struct()
                        .expect("should be a struct")
                        .members
                        .get_full(n.as_str())
                    {
                        let expected = *expected;
                        present[index] = true;
                        if let Some(actual) = self.evaluate_expr(&v) {
                            if !actual.is_coercible_to(self.context.types(), &expected) {
                                self.diagnostics.push(type_mismatch(
                                    self.context.types(),
                                    expected,
                                    n.span(),
                                    actual,
                                    v.span(),
                                ));
                            }
                        }
                    } else {
                        // Not a struct member
                        self.diagnostics
                            .push(not_a_struct_member(name.as_str(), &n));
                    }
                }

                // Find the first unspecified member that is required, if any
                let struct_type = self
                    .context
                    .types()
                    .type_definition(id)
                    .as_struct()
                    .expect("should be a struct");
                let mut unspecified = present
                    .iter()
                    .enumerate()
                    .filter_map(|(i, present)| {
                        if *present {
                            return None;
                        }

                        let (name, ty) = &struct_type.members.get_index(i).unwrap();
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

                    self.diagnostics
                        .push(missing_struct_members(&name, count, &members));
                }

                Some(ty)
            }
            Err(diagnostic) => {
                self.diagnostics.push(diagnostic);
                None
            }
        }
    }

    /// Evaluates a `runtime` section item.
    pub(crate) fn evaluate_runtime_item(&mut self, name: &Ident, expr: &Expr) {
        let expr_ty = self.evaluate_expr(expr).unwrap_or(Type::Union);
        if !self.evaluate_requirement(name, expr, expr_ty) {
            // Always use object types for `runtime` section `inputs` and `outputs` keys as
            // only `hints` sections can use input/output hidden types
            if let Some(expected) = task_hint_types(self.context.version(), name.as_str(), false) {
                if !expected
                    .iter()
                    .any(|target| expr_ty.is_coercible_to(self.context.types(), target))
                {
                    self.diagnostics.push(type_mismatch_custom(
                        self.context.types(),
                        expected,
                        name.span(),
                        expr_ty,
                        expr.span(),
                    ));
                }
            }
        }
    }

    /// Evaluates a `requirements` section item.
    pub(crate) fn evaluate_requirements_item(&mut self, name: &Ident, expr: &Expr) {
        let expr_ty = self.evaluate_expr(expr).unwrap_or(Type::Union);
        self.evaluate_requirement(name, expr, expr_ty);
    }

    /// Evaluates a requirement in either a `requirements` section or a legacy
    /// `runtime` section.
    ///
    /// Returns `true` if the name matched a requirement or `false` if it did
    /// not.
    fn evaluate_requirement(&mut self, name: &Ident, expr: &Expr, expr_ty: Type) -> bool {
        if let Some(expected) = task_requirement_types(self.context.version(), name.as_str()) {
            if !expected
                .iter()
                .any(|target| expr_ty.is_coercible_to(self.context.types(), target))
            {
                self.diagnostics.push(type_mismatch_custom(
                    self.context.types(),
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
    fn evaluate_literal_hints(&mut self, expr: &LiteralHints) -> Option<Type> {
        if !self.context.supports_hints_type() {
            return None;
        }

        for item in expr.items() {
            self.evaluate_hints_item(&item.name(), &item.expr())
        }

        Some(Type::Hints)
    }

    /// Evaluates a hints item, whether in task `hints` section or a `hints`
    /// literal expression.
    pub(crate) fn evaluate_hints_item(&mut self, name: &Ident, expr: &Expr) {
        let expr_ty = self.evaluate_expr(expr).unwrap_or(Type::Union);
        if let Some(expected) = task_hint_types(self.context.version(), name.as_str(), true) {
            if !expected
                .iter()
                .any(|target| expr_ty.is_coercible_to(self.context.types(), target))
            {
                self.diagnostics.push(type_mismatch_custom(
                    self.context.types(),
                    expected,
                    name.span(),
                    expr_ty,
                    expr.span(),
                ));
            }
        }
    }

    /// Evaluates the type of a literal input expression.
    fn evaluate_literal_input(&mut self, expr: &LiteralInput) -> Option<Type> {
        // Check to see if inputs literals are supported in the evaluation scope
        if !self.context.supports_input_type() {
            return None;
        }

        // Evaluate the items of the literal
        for item in expr.items() {
            self.evaluate_literal_io_item(item.names(), item.expr(), Io::Input);
        }

        Some(Type::Input)
    }

    /// Evaluates the type of a literal output expression.
    fn evaluate_literal_output(&mut self, expr: &LiteralOutput) -> Option<Type> {
        // Check to see if output literals are supported in the evaluation scope
        if !self.context.supports_output_type() {
            return None;
        }

        // Evaluate the items of the literal
        for item in expr.items() {
            self.evaluate_literal_io_item(item.names(), item.expr(), Io::Output);
        }

        Some(Type::Output)
    }

    /// Evaluates a literal input/output item.
    fn evaluate_literal_io_item(&mut self, names: impl Iterator<Item = Ident>, expr: Expr, io: Io) {
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
                    self.context.input(name.as_str()).map(|i| i.ty())
                } else {
                    self.context.output(name.as_str()).map(|o| o.ty())
                } {
                    Some(ty) => ty,
                    None => {
                        self.diagnostics.push(unknown_task_io(
                            self.context.task_name().expect("should have task name"),
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
                match s.members.get(name.as_str()) {
                    Some(ty) => *ty,
                    None => {
                        self.diagnostics.push(not_a_struct_member(&s.name, &name));
                        break;
                    }
                }
            };

            match ty {
                Type::Compound(ty)
                    if matches!(
                        self.context.types().type_definition(ty.definition()),
                        CompoundTypeDef::Struct(_)
                    ) =>
                {
                    s = Some(
                        self.context
                            .types()
                            .type_definition(ty.definition())
                            .as_struct()
                            .unwrap(),
                    );
                }
                _ if names.peek().is_some() => {
                    self.diagnostics.push(not_a_struct(&name, i == 0));
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
        if !expr_ty.is_coercible_to(self.context.types(), &Type::Hints) {
            self.diagnostics.push(type_mismatch(
                self.context.types(),
                Type::Hints,
                span.expect("should have span"),
                expr_ty,
                expr.span(),
            ));
        }
    }

    /// Evaluates the type of an `if` expression.
    fn evaluate_if_expr(&mut self, expr: &IfExpr) -> Option<Type> {
        let (cond_expr, true_expr, false_expr) = expr.exprs();

        // The conditional should be a boolean
        let cond_ty = self.evaluate_expr(&cond_expr).unwrap_or(Type::Union);
        if !cond_ty.is_coercible_to(self.context.types(), &PrimitiveTypeKind::Boolean.into()) {
            self.diagnostics.push(if_conditional_mismatch(
                self.context.types(),
                cond_ty,
                cond_expr.span(),
            ));
        }

        // Check that the two expressions have the same type
        let true_ty = self.evaluate_expr(&true_expr).unwrap_or(Type::Union);
        let false_ty = self.evaluate_expr(&false_expr).unwrap_or(Type::Union);

        match (true_ty, false_ty) {
            (Type::Union, Type::Union) => None,
            (Type::Union, _) => Some(false_ty),
            (_, Type::Union) => Some(true_ty),
            _ => {
                if let Some(ty) = true_ty.common_type(self.context.types(), false_ty) {
                    Some(ty)
                } else {
                    self.diagnostics.push(type_mismatch(
                        self.context.types(),
                        true_ty,
                        true_expr.span(),
                        false_ty,
                        false_expr.span(),
                    ));

                    None
                }
            }
        }
    }

    /// Evaluates the type of a `logical not` expression.
    fn evaluate_logical_not_expr(&mut self, expr: &LogicalNotExpr) -> Option<Type> {
        // The operand should be a boolean
        let operand = expr.operand();
        let ty = self.evaluate_expr(&operand).unwrap_or(Type::Union);
        if !ty.is_coercible_to(self.context.types(), &PrimitiveTypeKind::Boolean.into()) {
            self.diagnostics.push(logical_not_mismatch(
                self.context.types(),
                ty,
                operand.span(),
            ));
        }

        Some(PrimitiveTypeKind::Boolean.into())
    }

    /// Evaluates the type of a negation expression.
    fn evaluate_negation_expr(&mut self, expr: &NegationExpr) -> Option<Type> {
        // The operand should be a int or float
        let operand = expr.operand();
        let ty = self.evaluate_expr(&operand)?;

        // If the type is `Int`, treat it as `Int`
        // This is checked first as `Int` is coercible to `Float`
        if ty.type_eq(self.context.types(), &PrimitiveTypeKind::Integer.into()) {
            return Some(PrimitiveTypeKind::Integer.into());
        }

        if !ty.is_coercible_to(self.context.types(), &PrimitiveTypeKind::Float.into()) {
            self.diagnostics
                .push(negation_mismatch(self.context.types(), ty, operand.span()));
            // Type is indeterminate as the expression may evaluate to more than one type
            return None;
        }

        Some(PrimitiveTypeKind::Float.into())
    }

    /// Evaluates the type of a `logical or` expression.
    fn evaluate_logical_or_expr(&mut self, expr: &LogicalOrExpr) -> Option<Type> {
        // Both operands should be booleans
        let (lhs, rhs) = expr.operands();

        let ty = self.evaluate_expr(&lhs).unwrap_or(Type::Union);
        if !ty.is_coercible_to(self.context.types(), &PrimitiveTypeKind::Boolean.into()) {
            self.diagnostics
                .push(logical_or_mismatch(self.context.types(), ty, lhs.span()));
        }

        let ty = self.evaluate_expr(&rhs).unwrap_or(Type::Union);
        if !ty.is_coercible_to(self.context.types(), &PrimitiveTypeKind::Boolean.into()) {
            self.diagnostics
                .push(logical_or_mismatch(self.context.types(), ty, rhs.span()));
        }

        Some(PrimitiveTypeKind::Boolean.into())
    }

    /// Evaluates the type of a `logical and` expression.
    fn evaluate_logical_and_expr(&mut self, expr: &LogicalAndExpr) -> Option<Type> {
        // Both operands should be booleans
        let (lhs, rhs) = expr.operands();

        let ty = self.evaluate_expr(&lhs).unwrap_or(Type::Union);
        if !ty.is_coercible_to(self.context.types(), &PrimitiveTypeKind::Boolean.into()) {
            self.diagnostics
                .push(logical_and_mismatch(self.context.types(), ty, lhs.span()));
        }

        let ty = self.evaluate_expr(&rhs).unwrap_or(Type::Union);
        if !ty.is_coercible_to(self.context.types(), &PrimitiveTypeKind::Boolean.into()) {
            self.diagnostics
                .push(logical_and_mismatch(self.context.types(), ty, rhs.span()));
        }

        Some(PrimitiveTypeKind::Boolean.into())
    }

    /// Evaluates the type of a comparison expression.
    fn evaluate_comparison_expr(
        &mut self,
        op: ComparisonOperator,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
    ) -> Option<Type> {
        let lhs_ty = self.evaluate_expr(lhs).unwrap_or(Type::Union);
        let rhs_ty = self.evaluate_expr(rhs).unwrap_or(Type::Union);

        // Check for comparison to `None` or `Union` and allow it
        if lhs_ty.is_union() || lhs_ty.is_none() || rhs_ty.is_union() || rhs_ty.is_none() {
            return Some(PrimitiveTypeKind::Boolean.into());
        }

        // Check LHS and RHS for being coercible to one of the supported primitive types
        for expected in [
            Type::from(PrimitiveTypeKind::Boolean),
            PrimitiveTypeKind::Integer.into(),
            PrimitiveTypeKind::Float.into(),
            PrimitiveTypeKind::String.into(),
            PrimitiveTypeKind::File.into(),
            PrimitiveTypeKind::Directory.into(),
        ] {
            // Only support equality/inequality comparisons for `File` and `Directory`
            if op != ComparisonOperator::Equality
                && op != ComparisonOperator::Inequality
                && (matches!(
                    lhs_ty.as_primitive().map(|ty| ty.kind),
                    Some(PrimitiveTypeKind::File) | Some(PrimitiveTypeKind::Directory)
                ) || matches!(
                    rhs_ty.as_primitive().map(|ty| ty.kind),
                    Some(PrimitiveTypeKind::File) | Some(PrimitiveTypeKind::Directory)
                ))
            {
                continue;
            }

            if lhs_ty.is_coercible_to(self.context.types(), &expected)
                && rhs_ty.is_coercible_to(self.context.types(), &expected)
            {
                return Some(PrimitiveTypeKind::Boolean.into());
            }

            let expected = expected.optional();
            if lhs_ty.is_coercible_to(self.context.types(), &expected)
                && rhs_ty.is_coercible_to(self.context.types(), &expected)
            {
                return Some(PrimitiveTypeKind::Boolean.into());
            }
        }

        // For equality comparisons, check LHS and RHS being compound types
        if op == ComparisonOperator::Equality || op == ComparisonOperator::Inequality {
            // Check for object
            if (lhs_ty.is_coercible_to(self.context.types(), &Type::Object)
                && rhs_ty.is_coercible_to(self.context.types(), &Type::Object))
                || (lhs_ty.is_coercible_to(self.context.types(), &Type::OptionalObject)
                    && rhs_ty.is_coercible_to(self.context.types(), &Type::OptionalObject))
            {
                return Some(PrimitiveTypeKind::Boolean.into());
            }

            // Check for other compound types
            if let Type::Compound(lhs) = &lhs_ty {
                if let Type::Compound(rhs) = &rhs_ty {
                    if lhs.definition() == rhs.definition() {
                        return Some(PrimitiveTypeKind::Boolean.into());
                    }

                    let lhs = self.context.types().type_definition(lhs.definition());
                    let rhs = self.context.types().type_definition(rhs.definition());
                    let equal = match (lhs, rhs) {
                        (CompoundTypeDef::Array(a), CompoundTypeDef::Array(b)) => {
                            a.type_eq(self.context.types(), b)
                        }
                        (CompoundTypeDef::Pair(a), CompoundTypeDef::Pair(b)) => {
                            a.type_eq(self.context.types(), b)
                        }
                        (CompoundTypeDef::Map(a), CompoundTypeDef::Map(b)) => {
                            a.type_eq(self.context.types(), b)
                        }
                        // Struct is handled in the above definition id comparison
                        _ => false,
                    };

                    if equal {
                        return Some(PrimitiveTypeKind::Boolean.into());
                    }
                }
            }
        }

        // A type mismatch at this point
        self.diagnostics.push(comparison_mismatch(
            self.context.types(),
            op,
            span,
            lhs_ty,
            lhs.span(),
            rhs_ty,
            rhs.span(),
        ));
        Some(PrimitiveTypeKind::Boolean.into())
    }

    /// Evaluates the type of a numeric expression.
    fn evaluate_numeric_expr(
        &mut self,
        op: NumericOperator,
        span: Span,
        lhs: &Expr,
        rhs: &Expr,
    ) -> Option<Type> {
        let lhs_ty = self.evaluate_expr(lhs).unwrap_or(Type::Union);
        let rhs_ty = self.evaluate_expr(rhs).unwrap_or(Type::Union);

        // If both sides are `Int`, the result is `Int`
        if lhs_ty.type_eq(self.context.types(), &PrimitiveTypeKind::Integer.into())
            && rhs_ty.type_eq(self.context.types(), &PrimitiveTypeKind::Integer.into())
        {
            return Some(PrimitiveTypeKind::Integer.into());
        }

        // If both sides are coercible to `Float`, the result is `Float`
        if !lhs_ty.is_union()
            && lhs_ty.is_coercible_to(self.context.types(), &PrimitiveTypeKind::Float.into())
            && !rhs_ty.is_union()
            && rhs_ty.is_coercible_to(self.context.types(), &PrimitiveTypeKind::Float.into())
        {
            return Some(PrimitiveTypeKind::Float.into());
        }

        // For addition, also support `String` on one or both sides of any primitive
        // type that isn't `Boolean`; in placeholder expressions, allow the
        // other side to also be optional
        if op == NumericOperator::Addition {
            let allow_optional = self.placeholders > 0;
            let other = if (!lhs_ty.is_optional() || allow_optional)
                && lhs_ty
                    .as_primitive()
                    .map(|p| p.kind() == PrimitiveTypeKind::String)
                    .unwrap_or(false)
            {
                Some((lhs_ty.is_optional(), rhs_ty, rhs.span()))
            } else if (!rhs_ty.is_optional() || allow_optional)
                && rhs_ty
                    .as_primitive()
                    .map(|p| p.kind() == PrimitiveTypeKind::String)
                    .unwrap_or(false)
            {
                Some((rhs_ty.is_optional(), lhs_ty, lhs.span()))
            } else {
                None
            };

            if let Some((optional, other, span)) = other {
                if (!other.is_optional() || allow_optional)
                    && other
                        .as_primitive()
                        .map(|p| p.kind() != PrimitiveTypeKind::Boolean)
                        .unwrap_or(other.is_union() || (allow_optional && other.is_none()))
                {
                    let ty: Type = PrimitiveTypeKind::String.into();
                    if optional || other.is_optional() {
                        return Some(ty.optional());
                    }

                    return Some(ty);
                }

                self.diagnostics
                    .push(string_concat_mismatch(self.context.types(), other, span));
                return None;
            }
        }

        if !lhs_ty.is_union() && !rhs_ty.is_union() {
            self.diagnostics.push(numeric_mismatch(
                self.context.types(),
                op,
                span,
                lhs_ty,
                lhs.span(),
                rhs_ty,
                rhs.span(),
            ));
        }

        None
    }

    /// Evaluates the type of a call expression.
    fn evaluate_call_expr(&mut self, expr: &CallExpr) -> Option<Type> {
        let target = expr.target();
        match STDLIB.function(target.as_str()) {
            Some(f) => {
                let arguments: Vec<_> = expr
                    .arguments()
                    .map(|expr| self.evaluate_expr(&expr).unwrap_or(Type::Union))
                    .collect();

                let minimum_version = f.minimum_version();
                if minimum_version > self.context.version() {
                    self.diagnostics.push(unsupported_function(
                        minimum_version,
                        target.as_str(),
                        target.span(),
                    ));
                }

                match f.bind(self.context.types_mut(), &arguments) {
                    Ok(ty) => return Some(ty),
                    Err(FunctionBindError::TooFewArguments(minimum)) => {
                        self.diagnostics.push(too_few_arguments(
                            target.as_str(),
                            target.span(),
                            minimum,
                            arguments.len(),
                        ));
                    }
                    Err(FunctionBindError::TooManyArguments(maximum)) => {
                        self.diagnostics.push(too_many_arguments(
                            target.as_str(),
                            target.span(),
                            maximum,
                            arguments.len(),
                            expr.arguments().skip(maximum).map(|e| e.span()),
                        ));
                    }
                    Err(FunctionBindError::ArgumentTypeMismatch { index, expected }) => {
                        self.diagnostics.push(argument_type_mismatch(
                            self.context.types(),
                            target.as_str(),
                            &expected,
                            arguments[index],
                            expr.arguments()
                                .nth(index)
                                .map(|e| e.span())
                                .expect("should have span"),
                        ));
                    }
                    Err(FunctionBindError::Ambiguous { first, second }) => {
                        self.diagnostics.push(ambiguous_argument(
                            target.as_str(),
                            target.span(),
                            &first,
                            &second,
                        ));
                    }
                }

                Some(f.realize_unconstrained_return_type(self.context.types_mut(), &arguments))
            }
            None => {
                self.diagnostics
                    .push(unknown_function(target.as_str(), target.span()));
                None
            }
        }
    }

    /// Evaluates the type of an index expression.
    fn evaluate_index_expr(&mut self, expr: &IndexExpr) -> Option<Type> {
        let (target, index) = expr.operands();

        // Determine the expected index type and result type of the expression
        let target_ty = self.evaluate_expr(&target)?;
        let (expected_index_ty, result_ty) = match target_ty {
            Type::Compound(ty) => match self.context.types().type_definition(ty.definition()) {
                CompoundTypeDef::Array(ty) => (
                    Some(PrimitiveTypeKind::Integer.into()),
                    Some(ty.element_type()),
                ),
                CompoundTypeDef::Map(ty) => (Some(ty.key_type()), Some(ty.value_type())),
                _ => (None, None),
            },
            _ => (None, None),
        };

        // Check that the index type is the expected one
        if let Some(expected_index_ty) = expected_index_ty {
            let index_ty = self.evaluate_expr(&index).unwrap_or(Type::Union);
            if !index_ty.is_coercible_to(self.context.types(), &expected_index_ty) {
                self.diagnostics.push(index_type_mismatch(
                    self.context.types(),
                    expected_index_ty,
                    index_ty,
                    index.span(),
                ));
            }
        }

        match result_ty {
            Some(ty) => Some(ty),
            None => {
                self.diagnostics
                    .push(cannot_index(self.context.types(), target_ty, target.span()));
                None
            }
        }
    }

    /// Evaluates the type of an access expression.
    fn evaluate_access_expr(&mut self, expr: &AccessExpr) -> Option<Type> {
        let (target, name) = expr.operands();
        let ty = self.evaluate_expr(&target)?;

        if matches!(ty, Type::Task) {
            return match task_member_type(name.as_str()) {
                Some(ty) => Some(ty),
                None => {
                    self.diagnostics.push(not_a_task_member(&name));
                    return None;
                }
            };
        }

        // Check to see if it's a compound type
        if let Type::Compound(ty) = &ty {
            // Check to see if it's a struct.
            let definition = self.context.types().type_definition(ty.definition());
            if let CompoundTypeDef::Struct(ty) = definition {
                if let Some(ty) = ty.members.get(name.as_str()) {
                    return Some(*ty);
                }

                self.diagnostics.push(not_a_struct_member(ty.name(), &name));
                return None;
            }

            // Check to see if it's a `Pair`
            if let CompoundTypeDef::Pair(ty) = definition {
                // Support `left` and `right` accessors for pairs
                return match name.as_str() {
                    "left" => Some(ty.left_type),
                    "right" => Some(ty.right_type),
                    _ => {
                        self.diagnostics.push(not_a_pair_accessor(&name));
                        None
                    }
                };
            }

            // Check to see if it's a call
            if let CompoundTypeDef::Call(ty) = definition {
                if let Some(output) = ty.outputs().get(name.as_str()) {
                    return Some(output.ty());
                }

                self.diagnostics
                    .push(unknown_call_io(ty, &name, Io::Output));
                return None;
            }
        }

        // Check to see if it's coercible to object; if so, treat as `Union` as it's
        // indeterminate
        if ty.is_coercible_to(self.context.types(), &Type::OptionalObject) {
            return Some(Type::Union);
        }

        self.diagnostics
            .push(cannot_access(self.context.types(), ty, target.span()));
        None
    }
}

//! Implementation of an expression evaluator for 1.x WDL documents.

use std::cmp::Ordering;
use std::fmt::Write;
use std::iter::once;
use std::sync::Arc;

use indexmap::IndexMap;
use ordered_float::Pow;
use wdl_analysis::diagnostics::ambiguous_argument;
use wdl_analysis::diagnostics::argument_type_mismatch;
use wdl_analysis::diagnostics::cannot_access;
use wdl_analysis::diagnostics::cannot_coerce_to_string;
use wdl_analysis::diagnostics::cannot_index;
use wdl_analysis::diagnostics::comparison_mismatch;
use wdl_analysis::diagnostics::if_conditional_mismatch;
use wdl_analysis::diagnostics::index_type_mismatch;
use wdl_analysis::diagnostics::logical_and_mismatch;
use wdl_analysis::diagnostics::logical_not_mismatch;
use wdl_analysis::diagnostics::logical_or_mismatch;
use wdl_analysis::diagnostics::map_key_not_primitive;
use wdl_analysis::diagnostics::missing_struct_members;
use wdl_analysis::diagnostics::no_common_type;
use wdl_analysis::diagnostics::not_a_pair_accessor;
use wdl_analysis::diagnostics::not_a_struct_member;
use wdl_analysis::diagnostics::numeric_mismatch;
use wdl_analysis::diagnostics::too_few_arguments;
use wdl_analysis::diagnostics::too_many_arguments;
use wdl_analysis::diagnostics::type_mismatch_custom;
use wdl_analysis::diagnostics::unknown_function;
use wdl_analysis::diagnostics::unsupported_function;
use wdl_analysis::stdlib::FunctionBindError;
use wdl_analysis::stdlib::MAX_PARAMETERS;
use wdl_analysis::stdlib::STDLIB;
use wdl_analysis::types::ArrayType;
use wdl_analysis::types::Coercible as _;
use wdl_analysis::types::CompoundTypeDef;
use wdl_analysis::types::MapType;
use wdl_analysis::types::Optional;
use wdl_analysis::types::PairType;
use wdl_analysis::types::PrimitiveTypeKind;
use wdl_analysis::types::Type;
use wdl_analysis::types::TypeEq;
use wdl_analysis::types::Types;
use wdl_analysis::types::v1::ComparisonOperator;
use wdl_analysis::types::v1::ExprTypeEvaluator;
use wdl_analysis::types::v1::NumericOperator;
use wdl_ast::AstNode;
use wdl_ast::AstNodeExt;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::AccessExpr;
use wdl_ast::v1::CallExpr;
use wdl_ast::v1::Expr;
use wdl_ast::v1::IfExpr;
use wdl_ast::v1::IndexExpr;
use wdl_ast::v1::LiteralArray;
use wdl_ast::v1::LiteralExpr;
use wdl_ast::v1::LiteralMap;
use wdl_ast::v1::LiteralObject;
use wdl_ast::v1::LiteralPair;
use wdl_ast::v1::LiteralString;
use wdl_ast::v1::LiteralStringKind;
use wdl_ast::v1::LiteralStruct;
use wdl_ast::v1::LogicalAndExpr;
use wdl_ast::v1::LogicalNotExpr;
use wdl_ast::v1::LogicalOrExpr;
use wdl_ast::v1::NegationExpr;
use wdl_ast::v1::Placeholder;
use wdl_ast::v1::PlaceholderOption;
use wdl_ast::v1::StringPart;
use wdl_ast::v1::StrippedStringPart;
use wdl_ast::version::V1;

use crate::Array;
use crate::Coercible;
use crate::CompoundValue;
use crate::Map;
use crate::Object;
use crate::Pair;
use crate::PrimitiveValue;
use crate::Struct;
use crate::Value;
use crate::diagnostics::array_index_out_of_range;
use crate::diagnostics::division_by_zero;
use crate::diagnostics::exponent_not_in_range;
use crate::diagnostics::exponentiation_requirement;
use crate::diagnostics::float_not_in_range;
use crate::diagnostics::integer_negation_not_in_range;
use crate::diagnostics::integer_not_in_range;
use crate::diagnostics::map_key_not_found;
use crate::diagnostics::multiline_string_requirement;
use crate::diagnostics::not_an_object_member;
use crate::diagnostics::numeric_overflow;
use crate::diagnostics::struct_member_coercion_failed;

/// Represents context to an expression evaluator.
pub trait EvaluationContext {
    /// Gets the supported version of the document being evaluated.
    fn version(&self) -> SupportedVersion;

    /// Gets the types collection associated with the evaluation.
    fn types(&self) -> &Types;

    /// Gets the mutable types collection associated with the evaluation.
    fn types_mut(&mut self) -> &mut Types;

    /// Gets the value of the given name in scope.
    fn resolve_name(&self, name: &Ident) -> Result<Value, Diagnostic>;

    /// Resolves a type name to a type.
    fn resolve_type_name(&self, name: &Ident) -> Result<Type, Diagnostic>;

    /// Gets the value to return for a call to the `stdout` function.
    ///
    /// This is `Some` only when evaluating task outputs.
    fn stdout(&self) -> Option<Value>;

    /// Gets the value to return for a call to the `stderr` function.
    ///
    /// This is `Some` only when evaluating task outputs.
    fn stderr(&self) -> Option<Value>;
}

/// Represents a WDL expression evaluator.
#[derive(Debug)]
pub struct ExprEvaluator<'a, C> {
    /// The expression evaluation context.
    context: &'a mut C,
    /// The nested count of placeholder evaluation.
    ///
    /// This is incremented immediately before a placeholder expression is
    /// evaluated and decremented immediately after.
    ///
    /// If the count is non-zero, special evaluation behavior is enabled for
    /// string interpolation.
    placeholders: usize,
    /// Tracks whether or not a `None`-resulting expression was evaluated during
    /// a placeholder evaluation.
    evaluated_none: bool,
}

impl<'a, C: EvaluationContext> ExprEvaluator<'a, C> {
    /// Creates a new expression evaluator.
    pub fn new(context: &'a mut C) -> Self {
        Self {
            context,
            placeholders: 0,
            evaluated_none: false,
        }
    }

    /// Evaluates the given expression.
    pub fn evaluate_expr(&mut self, expr: &Expr) -> Result<Value, Diagnostic> {
        let value = match expr {
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
                self.evaluate_numeric_expr(NumericOperator::Addition, &lhs, &rhs, expr.span())
            }
            Expr::Subtraction(expr) => {
                let (lhs, rhs) = expr.operands();
                self.evaluate_numeric_expr(NumericOperator::Subtraction, &lhs, &rhs, expr.span())
            }
            Expr::Multiplication(expr) => {
                let (lhs, rhs) = expr.operands();
                self.evaluate_numeric_expr(NumericOperator::Multiplication, &lhs, &rhs, expr.span())
            }
            Expr::Division(expr) => {
                let (lhs, rhs) = expr.operands();
                self.evaluate_numeric_expr(NumericOperator::Division, &lhs, &rhs, expr.span())
            }
            Expr::Modulo(expr) => {
                let (lhs, rhs) = expr.operands();
                self.evaluate_numeric_expr(NumericOperator::Modulo, &lhs, &rhs, expr.span())
            }
            Expr::Exponentiation(expr) => {
                if self.context.version() < SupportedVersion::V1(V1::Two) {
                    return Err(exponentiation_requirement(expr.span()));
                }
                let (lhs, rhs) = expr.operands();
                self.evaluate_numeric_expr(NumericOperator::Exponentiation, &lhs, &rhs, expr.span())
            }
            Expr::Call(expr) => self.evaluate_call_expr(expr),
            Expr::Index(expr) => self.evaluate_index_expr(expr),
            Expr::Access(expr) => self.evaluate_access_expr(expr),
        }?;

        self.evaluated_none |= self.placeholders > 0 && value.is_none();
        Ok(value)
    }

    /// Evaluates a literal expression.
    fn evaluate_literal_expr(&mut self, expr: &LiteralExpr) -> Result<Value, Diagnostic> {
        match expr {
            LiteralExpr::Boolean(lit) => Ok(lit.value().into()),
            LiteralExpr::Integer(lit) => {
                // Check to see if this literal is a direct child of a negation expression; if
                // so, we want to negate the literal
                let (value, span) = match lit.syntax().parent() {
                    Some(parent) if parent.kind() == SyntaxKind::NegationExprNode => {
                        let start = parent.text_range().start().into();
                        (lit.negate(), Span::new(start, lit.span().end() - start))
                    }
                    _ => (lit.value(), lit.span()),
                };

                Ok(value.ok_or_else(|| integer_not_in_range(span))?.into())
            }
            LiteralExpr::Float(lit) => Ok(lit
                .value()
                .ok_or_else(|| float_not_in_range(lit.span()))?
                .into()),
            LiteralExpr::String(lit) => self.evaluate_literal_string(lit),
            LiteralExpr::Array(lit) => self.evaluate_literal_array(lit),
            LiteralExpr::Pair(lit) => self.evaluate_literal_pair(lit),
            LiteralExpr::Map(lit) => self.evaluate_literal_map(lit),
            LiteralExpr::Object(lit) => self.evaluate_literal_object(lit),
            LiteralExpr::Struct(lit) => self.evaluate_literal_struct(lit),
            LiteralExpr::None(_) => Ok(Value::None),
            LiteralExpr::Hints(_) | LiteralExpr::Input(_) | LiteralExpr::Output(_) => {
                todo!("implement for WDL 1.2 support")
            }
        }
    }

    /// Evaluates a placeholder into the given string buffer.
    fn evaluate_placeholder(
        &mut self,
        placeholder: &Placeholder,
        buffer: &mut String,
    ) -> Result<(), Diagnostic> {
        let expr = placeholder.expr();
        match self.evaluate_expr(&expr)? {
            Value::None => {
                if let Some(o) = placeholder.option().as_ref().and_then(|o| o.as_default()) {
                    buffer.push_str(&self.evaluate_literal_string(&o.value())?.unwrap_string())
                }
            }
            Value::Primitive(PrimitiveValue::Boolean(v)) => {
                match placeholder
                    .option()
                    .as_ref()
                    .and_then(|o| o.as_true_false())
                {
                    Some(o) => {
                        let (t, f) = o.values();
                        if v {
                            buffer.push_str(&self.evaluate_literal_string(&t)?.unwrap_string());
                        } else {
                            buffer.push_str(&self.evaluate_literal_string(&f)?.unwrap_string());
                        }
                    }
                    None => {
                        if v {
                            buffer.push_str("true");
                        } else {
                            buffer.push_str("false");
                        }
                    }
                }
            }
            Value::Primitive(PrimitiveValue::Integer(v)) => write!(buffer, "{v}").unwrap(),
            Value::Primitive(PrimitiveValue::Float(v)) => write!(buffer, "{v}").unwrap(),
            Value::Primitive(PrimitiveValue::String(s))
            | Value::Primitive(PrimitiveValue::File(s))
            | Value::Primitive(PrimitiveValue::Directory(s)) => buffer.push_str(&s),
            Value::Compound(CompoundValue::Array(v))
                if matches!(placeholder.option(), Some(PlaceholderOption::Sep(_)))
                    && v.elements()
                        .first()
                        .map(|e| !matches!(e, Value::None | Value::Compound(_)))
                        .unwrap_or(false) =>
            {
                let option = placeholder.option().unwrap().unwrap_sep();

                let sep = self
                    .evaluate_literal_string(&option.separator())?
                    .unwrap_string();
                for (i, v) in v.elements().iter().enumerate() {
                    if i > 0 {
                        buffer.push_str(&sep);
                    }

                    write!(buffer, "{v}").unwrap();
                }
            }
            Value::Compound(v) => {
                return Err(cannot_coerce_to_string(
                    self.context.types(),
                    v.ty(),
                    expr.span(),
                ));
            }
        }

        Ok(())
    }

    /// Evaluates a literal string expression.
    fn evaluate_literal_string(&mut self, expr: &LiteralString) -> Result<Value, Diagnostic> {
        /// Helper for evaluating placeholders in a string.
        /// This handles incrementing the nested placeholder count and handling
        /// failures when a `None` expression was evaluated.
        fn evaluate_placeholder<C: EvaluationContext>(
            evaluator: &mut ExprEvaluator<'_, C>,
            placeholder: &Placeholder,
            buffer: &mut String,
        ) -> Result<(), Diagnostic> {
            // Keep track of the start in case there is a `None` evaluated and an error
            let start = buffer.len();

            // Bump the placeholder count while evaluating the placeholder
            evaluator.placeholders += 1;
            let result = evaluator.evaluate_placeholder(placeholder, buffer);
            evaluator.placeholders -= 1;

            // Reset the evaluated none flag
            if evaluator.placeholders == 0 {
                let evaluated_none = std::mem::replace(&mut evaluator.evaluated_none, false);

                // If a `None` was evaluated and an error occurred, truncate to the start of the
                // placeholder evaluation
                if evaluated_none && result.is_err() {
                    buffer.truncate(start);
                    return Ok(());
                }
            }

            result
        }

        if expr.kind() == LiteralStringKind::Multiline
            && self.context.version() < SupportedVersion::V1(V1::Two)
        {
            return Err(multiline_string_requirement(expr.span()));
        }

        let mut s = String::new();
        if let Some(parts) = expr.strip_whitespace() {
            for part in parts {
                match part {
                    StrippedStringPart::Text(t) => s.push_str(t.as_str()),
                    StrippedStringPart::Placeholder(placeholder) => {
                        evaluate_placeholder(self, &placeholder, &mut s)?;
                    }
                }
            }
        } else {
            for part in expr.parts() {
                match part {
                    StringPart::Text(t) => s.push_str(t.as_str()),
                    StringPart::Placeholder(placeholder) => {
                        evaluate_placeholder(self, &placeholder, &mut s)?;
                    }
                }
            }
        }

        Ok(PrimitiveValue::new_string(s).into())
    }

    /// Evaluates a literal array expression.
    fn evaluate_literal_array(&mut self, expr: &LiteralArray) -> Result<Value, Diagnostic> {
        // Look at the first array element to determine the element type
        // The remaining elements must have a common type
        let mut elements = expr.elements();
        let (element_ty, values) = match elements.next() {
            Some(expr) => {
                let mut values = Vec::new();
                let value = self.evaluate_expr(&expr)?;
                let mut expected: Type = value.ty();
                let mut expected_span = expr.span();
                values.push(value);

                // Ensure the remaining element types share a common type
                for expr in elements {
                    let value = self.evaluate_expr(&expr)?;
                    let actual = value.ty();

                    if let Some(ty) = actual.common_type(self.context.types(), expected) {
                        expected = ty;
                        expected_span = expr.span();
                    } else {
                        return Err(no_common_type(
                            self.context.types(),
                            expected,
                            expected_span,
                            actual,
                            expr.span(),
                        ));
                    }

                    values.push(value);
                }

                (expected, values)
            }
            None => (Type::Union, Vec::new()),
        };

        let ty = self
            .context
            .types_mut()
            .add_array(ArrayType::new(element_ty));
        Ok(Array::new(self.context.types(), ty, values)
            .expect("array elements should coerce")
            .into())
    }

    /// Evaluates a literal pair expression.
    fn evaluate_literal_pair(&mut self, expr: &LiteralPair) -> Result<Value, Diagnostic> {
        let (left, right) = expr.exprs();
        let left = self.evaluate_expr(&left)?;
        let right = self.evaluate_expr(&right)?;
        let ty = self
            .context
            .types_mut()
            .add_pair(PairType::new(left.ty(), right.ty()));
        Ok(Pair::new(self.context.types(), ty, left, right)
            .expect("types should coerce")
            .into())
    }

    /// Evaluates a literal map expression.
    fn evaluate_literal_map(&mut self, expr: &LiteralMap) -> Result<Value, Diagnostic> {
        let mut items = expr.items();
        let (key_ty, value_ty, elements) = match items.next() {
            Some(item) => {
                let mut elements = Vec::new();

                // Evaluate the first key-value pair
                let (key, value) = item.key_value();
                let expected_key = self.evaluate_expr(&key)?;
                let mut expected_key_ty = expected_key.ty();
                let mut expected_key_span = key.span();
                let expected_value = self.evaluate_expr(&value)?;
                let mut expected_value_ty = expected_value.ty();
                let mut expected_value_span = value.span();

                // The key type must be primitive
                match expected_key {
                    Value::Primitive(key) => {
                        elements.push((key, expected_value));
                    }
                    _ => {
                        return Err(map_key_not_primitive(
                            self.context.types(),
                            key.span(),
                            expected_key.ty(),
                        ));
                    }
                }

                // Ensure the remaining items types share common types
                for item in items {
                    let (key, value) = item.key_value();
                    let actual_key = self.evaluate_expr(&key)?;
                    let actual_key_ty = actual_key.ty();
                    let actual_value = self.evaluate_expr(&value)?;
                    let actual_value_ty = actual_value.ty();

                    if let Some(ty) =
                        actual_key_ty.common_type(self.context.types(), expected_key_ty)
                    {
                        expected_key_ty = ty;
                        expected_key_span = key.span();
                    } else {
                        // No common key type
                        return Err(no_common_type(
                            self.context.types(),
                            expected_key_ty,
                            expected_key_span,
                            actual_key_ty,
                            key.span(),
                        ));
                    }

                    if let Some(ty) =
                        actual_value_ty.common_type(self.context.types(), expected_value_ty)
                    {
                        expected_value_ty = ty;
                        expected_value_span = value.span();
                    } else {
                        // No common value type
                        return Err(no_common_type(
                            self.context.types(),
                            expected_value_ty,
                            expected_value_span,
                            actual_value_ty,
                            value.span(),
                        ));
                    }

                    match actual_key {
                        Value::Primitive(key) => {
                            elements.push((key, actual_value));
                        }
                        _ => panic!("the key type is not primitive, but had a common type"),
                    }
                }

                (expected_key_ty, expected_value_ty, elements)
            }
            None => (Type::Union, Type::Union, Vec::new()),
        };

        let ty = self
            .context
            .types_mut()
            .add_map(MapType::new(key_ty, value_ty));
        Ok(Map::new(self.context.types(), ty, elements)
            .expect("map elements should coerce")
            .into())
    }

    /// Evaluates a literal object expression.
    fn evaluate_literal_object(&mut self, expr: &LiteralObject) -> Result<Value, Diagnostic> {
        Ok(Object::from(
            expr.items()
                .map(|item| {
                    let (name, value) = item.name_value();
                    Ok((name.as_str().to_string(), self.evaluate_expr(&value)?))
                })
                .collect::<Result<IndexMap<_, _>, _>>()?,
        )
        .into())
    }

    /// Evaluates a literal struct expression.
    fn evaluate_literal_struct(&mut self, expr: &LiteralStruct) -> Result<Value, Diagnostic> {
        let name = expr.name();
        let ty = self.context.resolve_type_name(&name)?;

        // Evaluate the members
        let mut members =
            IndexMap::with_capacity(self.context.types().struct_type(ty).members().len());
        for item in expr.items() {
            let (n, v) = item.name_value();
            if let Some(expected) = self
                .context
                .types()
                .struct_type(ty)
                .members()
                .get(n.as_str())
            {
                let expected = *expected;
                let value = self.evaluate_expr(&v)?;
                let value = value.coerce(self.context.types(), expected).map_err(|e| {
                    struct_member_coercion_failed(
                        self.context.types(),
                        &e,
                        expected,
                        n.span(),
                        value.ty(),
                        v.span(),
                    )
                })?;

                members.insert(n.as_str().to_string(), value);
            } else {
                // Not a struct member
                return Err(not_a_struct_member(name.as_str(), &n));
            }
        }

        let mut iter = self.context.types().struct_type(ty).members().iter();
        while let Some((n, ty)) = iter.next() {
            // Check for optional members that should be set to `None`
            if ty.is_optional() {
                if !members.contains_key(n) {
                    members.insert(n.clone(), Value::None);
                }
            } else {
                // Check for a missing required member
                if !members.contains_key(n) {
                    let mut missing = once(n)
                        .chain(iter.filter_map(|(n, ty)| {
                            if ty.is_optional() && !members.contains_key(n.as_str()) {
                                Some(n)
                            } else {
                                None
                            }
                        }))
                        .peekable();
                    let mut names: String = String::new();
                    let mut count = 0;
                    while let Some(n) = missing.next() {
                        match (missing.peek().is_none(), count) {
                            (true, c) if c > 1 => names.push_str(", and "),
                            (true, 1) => names.push_str(" and "),
                            (false, c) if c > 0 => names.push_str(", "),
                            _ => {}
                        }

                        write!(&mut names, "`{n}`").ok();
                        count += 1;
                    }

                    return Err(missing_struct_members(&name, count, &names));
                }
            }
        }

        Ok(Struct::new_unchecked(
            ty,
            self.context.types().struct_type(ty).name().clone(),
            Arc::new(members),
        )
        .into())
    }

    /// Evaluates an `if` expression.
    fn evaluate_if_expr(&mut self, expr: &IfExpr) -> Result<Value, Diagnostic> {
        /// Used to translate an expression evaluation context to an expression
        /// type evaluation context.
        struct TypeContext<'a, C: EvaluationContext>(&'a mut C);
        impl<'a, C: EvaluationContext> wdl_analysis::types::v1::EvaluationContext for TypeContext<'a, C> {
            fn version(&self) -> SupportedVersion {
                self.0.version()
            }

            fn types(&self) -> &wdl_analysis::types::Types {
                self.0.types()
            }

            fn types_mut(&mut self) -> &mut wdl_analysis::types::Types {
                self.0.types_mut()
            }

            fn resolve_name(&self, name: &wdl_ast::Ident) -> Option<Type> {
                self.0.resolve_name(name).map(|v| v.ty()).ok()
            }

            fn resolve_type_name(&mut self, name: &wdl_ast::Ident) -> Result<Type, Diagnostic> {
                self.0.resolve_type_name(name)
            }

            fn input(&self, _name: &str) -> Option<wdl_analysis::document::Input> {
                todo!("implement for WDL 1.2 support")
            }

            fn output(&self, _name: &str) -> Option<wdl_analysis::document::Output> {
                todo!("implement for WDL 1.2 support")
            }

            fn task_name(&self) -> Option<&str> {
                todo!("implement for WDL 1.2 support")
            }

            fn supports_hints_type(&self) -> bool {
                todo!("implement for WDL 1.2 support")
            }

            fn supports_input_type(&self) -> bool {
                todo!("implement for WDL 1.2 support")
            }

            fn supports_output_type(&self) -> bool {
                todo!("implement for WDL 1.2 support")
            }
        }

        let (cond_expr, true_expr, false_expr) = expr.exprs();

        // Evaluate the conditional expression and the true expression or the false
        // expression, depending on the result of the conditional expression
        let mut diagnostics = Vec::new();
        let cond = self.evaluate_expr(&cond_expr)?;
        let (value, true_ty, false_ty) = if cond
            .coerce(self.context.types(), PrimitiveTypeKind::Boolean.into())
            .map_err(|_| {
                if_conditional_mismatch(self.context.types(), cond.ty(), cond_expr.span())
            })?
            .unwrap_boolean()
        {
            // Evaluate the `true` expression and calculate the type of the `false`
            // expression
            let value = self.evaluate_expr(&true_expr)?;
            let true_ty = value.ty();
            let false_ty = ExprTypeEvaluator::new(&mut TypeContext(self.context), &mut diagnostics)
                .evaluate_expr(&false_expr)
                .unwrap_or(Type::Union);
            (value, true_ty, false_ty)
        } else {
            // Evaluate the `false` expression and calculate the type of the `true`
            // expression
            let value = self.evaluate_expr(&false_expr)?;
            let true_ty = ExprTypeEvaluator::new(&mut TypeContext(self.context), &mut diagnostics)
                .evaluate_expr(&true_expr)
                .unwrap_or(Type::Union);
            let false_ty = value.ty();
            (value, true_ty, false_ty)
        };

        if let Some(diagnostic) = diagnostics.pop() {
            return Err(diagnostic);
        }

        // Determine the common type of the true and false expressions
        // The value must be coerced to that type
        let ty = false_ty
            .common_type(self.context.types(), true_ty)
            .ok_or_else(|| {
                no_common_type(
                    self.context.types(),
                    true_ty,
                    true_expr.span(),
                    false_ty,
                    false_expr.span(),
                )
            })?;

        Ok(value
            .coerce(self.context.types(), ty)
            .expect("coercion should not fail"))
    }

    /// Evaluates a `logical not` expression.
    fn evaluate_logical_not_expr(&mut self, expr: &LogicalNotExpr) -> Result<Value, Diagnostic> {
        // The operand should be a boolean
        let operand = expr.operand();
        let value = self.evaluate_expr(&operand)?;
        Ok((!value
            .coerce(self.context.types(), PrimitiveTypeKind::Boolean.into())
            .map_err(|_| logical_not_mismatch(self.context.types(), value.ty(), operand.span()))?
            .unwrap_boolean())
        .into())
    }

    /// Evaluates a negation expression.
    fn evaluate_negation_expr(&mut self, expr: &NegationExpr) -> Result<Value, Diagnostic> {
        let operand = expr.operand();
        let value = self.evaluate_expr(&operand)?;
        let ty = value.ty();

        // If the type is `Int`, treat it as `Int`
        if ty.type_eq(self.context.types(), &PrimitiveTypeKind::Integer.into()) {
            return match operand {
                Expr::Literal(LiteralExpr::Integer(_)) => {
                    // Already negated during integer literal evaluation
                    Ok(value)
                }
                _ => {
                    let value = value.unwrap_integer();
                    Ok(value
                        .checked_neg()
                        .ok_or_else(|| integer_negation_not_in_range(value, operand.span()))?
                        .into())
                }
            };
        }

        // If the type is `Float`, treat it as `Float`
        if ty.type_eq(self.context.types(), &PrimitiveTypeKind::Float.into()) {
            let value = value.unwrap_float();
            return Ok((-value).into());
        }

        // Expected either `Int` or `Float`
        Err(type_mismatch_custom(
            self.context.types(),
            &[
                PrimitiveTypeKind::Integer.into(),
                PrimitiveTypeKind::Float.into(),
            ],
            operand.span(),
            ty,
            operand.span(),
        ))
    }

    /// Evaluates a `logical or` expression.
    fn evaluate_logical_or_expr(&mut self, expr: &LogicalOrExpr) -> Result<Value, Diagnostic> {
        let (lhs, rhs) = expr.operands();

        // Evaluate the left-hand side first
        let left = self.evaluate_expr(&lhs)?;
        if left
            .coerce(self.context.types(), PrimitiveTypeKind::Boolean.into())
            .map_err(|_| logical_or_mismatch(self.context.types(), left.ty(), lhs.span()))?
            .unwrap_boolean()
        {
            // Short-circuit if the left-hand side is true
            return Ok(true.into());
        }

        // Otherwise, evaluate the right-hand side
        let right = self.evaluate_expr(&rhs)?;
        right
            .coerce(self.context.types(), PrimitiveTypeKind::Boolean.into())
            .map_err(|_| logical_or_mismatch(self.context.types(), right.ty(), rhs.span()))
    }

    /// Evaluates a `logical and` expression.
    fn evaluate_logical_and_expr(&mut self, expr: &LogicalAndExpr) -> Result<Value, Diagnostic> {
        let (lhs, rhs) = expr.operands();

        // Evaluate the left-hand side first
        let left = self.evaluate_expr(&lhs)?;
        if !left
            .coerce(self.context.types(), PrimitiveTypeKind::Boolean.into())
            .map_err(|_| logical_and_mismatch(self.context.types(), left.ty(), lhs.span()))?
            .unwrap_boolean()
        {
            // Short-circuit if the left-hand side is false
            return Ok(false.into());
        }

        // Otherwise, evaluate the right-hand side
        let right = self.evaluate_expr(&rhs)?;
        right
            .coerce(self.context.types(), PrimitiveTypeKind::Boolean.into())
            .map_err(|_| logical_and_mismatch(self.context.types(), right.ty(), rhs.span()))
    }

    /// Evaluates a comparison expression.
    fn evaluate_comparison_expr(
        &mut self,
        op: ComparisonOperator,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let left = self.evaluate_expr(lhs)?;
        let right = self.evaluate_expr(rhs)?;

        match op {
            ComparisonOperator::Equality => Value::equals(self.context.types(), &left, &right),
            ComparisonOperator::Inequality => {
                Value::equals(self.context.types(), &left, &right).map(|r| !r)
            }
            ComparisonOperator::Less
            | ComparisonOperator::LessEqual
            | ComparisonOperator::Greater
            | ComparisonOperator::GreaterEqual => {
                // Only primitive types support other comparisons
                match (&left, &right) {
                    (Value::Primitive(left), Value::Primitive(right)) => {
                        PrimitiveValue::compare(left, right).map(|o| match o {
                            Ordering::Less => matches!(
                                op,
                                ComparisonOperator::Less | ComparisonOperator::LessEqual
                            ),
                            Ordering::Equal => matches!(
                                op,
                                ComparisonOperator::LessEqual | ComparisonOperator::GreaterEqual
                            ),
                            Ordering::Greater => matches!(
                                op,
                                ComparisonOperator::Greater | ComparisonOperator::GreaterEqual
                            ),
                        })
                    }
                    _ => None,
                }
            }
        }
        .map(Into::into)
        .ok_or_else(|| {
            comparison_mismatch(
                self.context.types(),
                op,
                span,
                left.ty(),
                lhs.span(),
                right.ty(),
                rhs.span(),
            )
        })
    }

    /// Evaluates a numeric expression.
    fn evaluate_numeric_expr(
        &mut self,
        op: NumericOperator,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        /// Implements numeric operations on integer operands.
        fn int_numeric_op(
            op: NumericOperator,
            left: i64,
            right: i64,
            span: Span,
            rhs_span: Span,
        ) -> Result<i64, Diagnostic> {
            match op {
                NumericOperator::Addition => left
                    .checked_add(right)
                    .ok_or_else(|| numeric_overflow(span)),
                NumericOperator::Subtraction => left
                    .checked_sub(right)
                    .ok_or_else(|| numeric_overflow(span)),
                NumericOperator::Multiplication => left
                    .checked_mul(right)
                    .ok_or_else(|| numeric_overflow(span)),
                NumericOperator::Division => {
                    if right == 0 {
                        return Err(division_by_zero(span, rhs_span));
                    }

                    left.checked_div(right)
                        .ok_or_else(|| numeric_overflow(span))
                }
                NumericOperator::Modulo => {
                    if right == 0 {
                        return Err(division_by_zero(span, rhs_span));
                    }

                    left.checked_rem(right)
                        .ok_or_else(|| numeric_overflow(span))
                }
                NumericOperator::Exponentiation => left
                    .checked_pow(
                        (right)
                            .try_into()
                            .map_err(|_| exponent_not_in_range(rhs_span))?,
                    )
                    .ok_or_else(|| numeric_overflow(span)),
            }
        }

        /// Implements numeric operations on floating point operands.
        fn float_numeric_op(op: NumericOperator, left: f64, right: f64) -> f64 {
            match op {
                NumericOperator::Addition => left + right,
                NumericOperator::Subtraction => left - right,
                NumericOperator::Multiplication => left * right,
                NumericOperator::Division => left / right,
                NumericOperator::Modulo => left % right,
                NumericOperator::Exponentiation => left.pow(right),
            }
        }

        let left = self.evaluate_expr(lhs)?;
        let right = self.evaluate_expr(rhs)?;
        match (&left, &right) {
            (
                Value::Primitive(PrimitiveValue::Integer(left)),
                Value::Primitive(PrimitiveValue::Integer(right)),
            ) => Some(int_numeric_op(op, *left, *right, span, rhs.span())?.into()),
            (
                Value::Primitive(PrimitiveValue::Float(left)),
                Value::Primitive(PrimitiveValue::Float(right)),
            ) => Some(float_numeric_op(op, left.0, right.0).into()),
            (
                Value::Primitive(PrimitiveValue::Integer(left)),
                Value::Primitive(PrimitiveValue::Float(right)),
            ) => Some(float_numeric_op(op, *left as f64, right.0).into()),
            (
                Value::Primitive(PrimitiveValue::Float(left)),
                Value::Primitive(PrimitiveValue::Integer(right)),
            ) => Some(float_numeric_op(op, left.0, *right as f64).into()),
            (Value::Primitive(PrimitiveValue::String(left)), Value::Primitive(right))
                if op == NumericOperator::Addition
                    && !matches!(right, PrimitiveValue::Boolean(_)) =>
            {
                let s = match right {
                    PrimitiveValue::Boolean(_) => unreachable!(),
                    PrimitiveValue::Integer(v) => format!("{left}{v}"),
                    PrimitiveValue::Float(v) => format!("{left}{v}"),
                    PrimitiveValue::String(v)
                    | PrimitiveValue::File(v)
                    | PrimitiveValue::Directory(v) => format!("{left}{v}"),
                };

                Some(PrimitiveValue::new_string(s).into())
            }
            (Value::Primitive(left), Value::Primitive(PrimitiveValue::String(right)))
                if op == NumericOperator::Addition
                    && !matches!(left, PrimitiveValue::Boolean(_)) =>
            {
                let s = match left {
                    PrimitiveValue::Boolean(_) => unreachable!(),
                    PrimitiveValue::Integer(v) => format!("{v}{right}"),
                    PrimitiveValue::Float(v) => format!("{v}{right}"),
                    PrimitiveValue::String(v)
                    | PrimitiveValue::File(v)
                    | PrimitiveValue::Directory(v) => format!("{v}{right}"),
                };

                Some(PrimitiveValue::new_string(s).into())
            }
            (Value::Primitive(PrimitiveValue::String(_)), Value::None)
            | (Value::None, Value::Primitive(PrimitiveValue::String(_)))
                if op == NumericOperator::Addition && self.placeholders > 0 =>
            {
                // Allow string concatenation with `None` in placeholders, which evaluates to
                // `None`
                Some(Value::None)
            }
            _ => None,
        }
        .ok_or_else(|| {
            numeric_mismatch(
                self.context.types(),
                op,
                span,
                left.ty(),
                lhs.span(),
                right.ty(),
                rhs.span(),
            )
        })
    }

    /// Evaluates a call expression.
    fn evaluate_call_expr(&mut self, expr: &CallExpr) -> Result<Value, Diagnostic> {
        let target = expr.target();
        match STDLIB.function(target.as_str()) {
            Some(f) => {
                let minimum_version = f.minimum_version();
                if minimum_version > self.context.version() {
                    return Err(unsupported_function(
                        minimum_version,
                        target.as_str(),
                        target.span(),
                    ));
                }

                let mut count = 0;
                let mut types = [Type::Union; MAX_PARAMETERS];
                let mut arguments = [const { Value::None }; MAX_PARAMETERS];

                // Evaluate the argument expressions
                for expr in expr.arguments() {
                    let value = self.evaluate_expr(&expr)?;
                    types[count] = value.ty();
                    arguments[count] = value;
                    count += 1;
                }

                // Bind the function based on the argument types
                match f.bind(self.context.types_mut(), &types[0..count]) {
                    Ok(_) => {
                        todo!("implement function calls")
                    }
                    Err(FunctionBindError::TooFewArguments(minimum)) => Err(too_few_arguments(
                        target.as_str(),
                        target.span(),
                        minimum,
                        arguments.len(),
                    )),
                    Err(FunctionBindError::TooManyArguments(maximum)) => Err(too_many_arguments(
                        target.as_str(),
                        target.span(),
                        maximum,
                        arguments.len(),
                        expr.arguments().skip(maximum).map(|e| e.span()),
                    )),
                    Err(FunctionBindError::ArgumentTypeMismatch { index, expected }) => {
                        Err(argument_type_mismatch(
                            self.context.types(),
                            target.as_str(),
                            &expected,
                            types[index],
                            expr.arguments()
                                .nth(index)
                                .map(|e| e.span())
                                .expect("should have span"),
                        ))
                    }
                    Err(FunctionBindError::Ambiguous { first, second }) => Err(ambiguous_argument(
                        target.as_str(),
                        target.span(),
                        &first,
                        &second,
                    )),
                }
            }
            None => Err(unknown_function(target.as_str(), target.span())),
        }
    }

    /// Evaluates the type of an index expression.
    fn evaluate_index_expr(&mut self, expr: &IndexExpr) -> Result<Value, Diagnostic> {
        let (target, index) = expr.operands();
        match self.evaluate_expr(&target)? {
            Value::Compound(CompoundValue::Array(array)) => match self.evaluate_expr(&index)? {
                Value::Primitive(PrimitiveValue::Integer(i)) => {
                    match i.try_into().map(|i: usize| array.elements().get(i)) {
                        Ok(Some(value)) => Ok(value.clone()),
                        _ => Err(array_index_out_of_range(
                            i,
                            array.elements().len(),
                            index.span(),
                            target.span(),
                        )),
                    }
                }
                value => Err(index_type_mismatch(
                    self.context.types(),
                    PrimitiveTypeKind::Integer.into(),
                    value.ty(),
                    index.span(),
                )),
            },
            Value::Compound(CompoundValue::Map(map)) => {
                let ty = map.ty().as_compound().expect("type should be compound");
                let key_type = match self.context.types().type_definition(ty.definition()) {
                    CompoundTypeDef::Map(ty) => ty
                        .key_type()
                        .as_primitive()
                        .expect("type should be primitive"),
                    _ => panic!("expected a map type"),
                };

                match self.evaluate_expr(&index)? {
                    Value::Primitive(i)
                        if i.ty()
                            .is_coercible_to(self.context.types(), &key_type.into()) =>
                    {
                        match map.elements().get(&i) {
                            Some(value) => Ok(value.clone()),
                            None => Err(map_key_not_found(index.span())),
                        }
                    }
                    value => Err(index_type_mismatch(
                        self.context.types(),
                        key_type.into(),
                        value.ty(),
                        index.span(),
                    )),
                }
            }
            value => Err(cannot_index(
                self.context.types(),
                value.ty(),
                target.span(),
            )),
        }
    }

    /// Evaluates the type of an access expression.
    fn evaluate_access_expr(&mut self, expr: &AccessExpr) -> Result<Value, Diagnostic> {
        let (target, name) = expr.operands();

        // TODO: implement support for task values (required for WDL 1.2 support)
        // TODO: add support for access to call outputs

        match self.evaluate_expr(&target)? {
            Value::Compound(CompoundValue::Pair(pair)) => match name.as_str() {
                "left" => Ok(pair.left().clone()),
                "right" => Ok(pair.right().clone()),
                _ => Err(not_a_pair_accessor(&name)),
            },
            Value::Compound(CompoundValue::Struct(s)) => match s.members().get(name.as_str()) {
                Some(value) => Ok(value.clone()),
                None => Err(not_a_struct_member(
                    self.context.types().struct_type(s.ty()).name(),
                    &name,
                )),
            },
            Value::Compound(CompoundValue::Object(object)) => {
                match object.members().get(name.as_str()) {
                    Some(value) => Ok(value.clone()),
                    None => Err(not_an_object_member(&name)),
                }
            }
            value => Err(cannot_access(
                self.context.types(),
                value.ty(),
                target.span(),
            )),
        }
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use pretty_assertions::assert_eq;
    use wdl_analysis::diagnostics::unknown_name;
    use wdl_analysis::diagnostics::unknown_type;
    use wdl_analysis::types::StructType;
    use wdl_grammar::construct_tree;
    use wdl_grammar::grammar::v1;
    use wdl_grammar::lexer::Lexer;

    use super::*;
    use crate::ScopeRef;
    use crate::eval::Scope;

    /// Represents test evaluation context to an expression evaluator.
    #[derive(Debug)]
    pub struct TestEvaluationContext<'a> {
        /// The types collection.
        types: &'a mut Types,
        /// The supported version of WDL being evaluated.
        version: SupportedVersion,
        /// The map of known struct types.
        structs: HashMap<&'static str, Type>,
        /// The current evaluation scope.
        scope: ScopeRef<'a>,
        /// The stdout value from a task's execution.
        stdout: Option<Value>,
        /// The stderr value from a task's execution.
        stderr: Option<Value>,
    }

    impl<'a> TestEvaluationContext<'a> {
        /// Constructs a test evaluation context.
        pub fn new(version: SupportedVersion, types: &'a mut Types, scope: ScopeRef<'a>) -> Self {
            Self {
                types,
                version,
                structs: HashMap::new(),
                scope,
                stdout: None,
                stderr: None,
            }
        }
    }

    impl EvaluationContext for TestEvaluationContext<'_> {
        fn version(&self) -> SupportedVersion {
            self.version
        }

        fn types(&self) -> &Types {
            &self.types
        }

        fn types_mut(&mut self) -> &mut Types {
            &mut self.types
        }

        fn resolve_name(&self, name: &Ident) -> Result<Value, Diagnostic> {
            self.scope
                .lookup(name.as_str())
                .map(|v| v.clone())
                .ok_or_else(|| unknown_name(name.as_str(), name.span()))
        }

        fn resolve_type_name(&self, name: &Ident) -> Result<Type, Diagnostic> {
            self.structs
                .get(name.as_str())
                .copied()
                .ok_or_else(|| unknown_type(name.as_str(), name.span()))
        }

        fn stdout(&self) -> Option<Value> {
            self.stdout.clone()
        }

        fn stderr(&self) -> Option<Value> {
            self.stderr.clone()
        }
    }

    /// Evaluates a WDL v1 expression and returns the value or a
    /// parse/evaluation diagnostic.
    fn eval_v1_expr(
        version: V1,
        source: &str,
        types: &mut Types,
        scope: ScopeRef<'_>,
    ) -> Result<Value, Diagnostic> {
        eval_v1_expr_with_context(
            source,
            &mut TestEvaluationContext::new(SupportedVersion::V1(version), types, scope),
        )
    }

    /// Evaluates a WDL v1 expression and returns the value or a
    /// parse/evaluation diagnostic.
    fn eval_v1_expr_with_context(
        source: &str,
        context: &mut TestEvaluationContext<'_>,
    ) -> Result<Value, Diagnostic> {
        let source = source.trim();
        let mut parser = v1::Parser::new(Lexer::new(source));
        let marker = parser.start();
        match v1::expr(&mut parser, marker) {
            Ok(()) => {
                // This call to `next` is important as `next` adds any remaining buffered events
                assert!(
                    parser.next().is_none(),
                    "parser is not finished; expected a single expression with no remaining tokens"
                );
                let output = parser.finish();
                assert_eq!(
                    output.diagnostics.iter().next(),
                    None,
                    "the provided WDL source failed to parse"
                );
                let expr = Expr::cast(construct_tree(source, output.events))
                    .expect("should be an expression");
                let mut evaluator = ExprEvaluator::new(context);
                evaluator.evaluate_expr(&expr)
            }
            Err((marker, diagnostic)) => {
                marker.abandon(&mut parser);
                Err(diagnostic)
            }
        }
    }

    #[test]
    fn literal_none_expr() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Two, "None", &mut types, scope).unwrap();
        assert_eq!(value.to_string(), "None");
    }

    #[test]
    fn literal_bool_expr() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Two, "true", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_boolean(), true);

        let value = eval_v1_expr(V1::Two, "false", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_boolean(), false);
    }

    #[test]
    fn literal_int_expr() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Two, "12345", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 12345);

        let value = eval_v1_expr(V1::Two, "-54321", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), -54321);

        let value = eval_v1_expr(V1::Two, "0xdeadbeef", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 0xDEADBEEF);

        let value = eval_v1_expr(V1::Two, "0777", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 0o777);

        let value = eval_v1_expr(V1::Two, "-9223372036854775808", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), -9223372036854775808);

        let diagnostic = eval_v1_expr(V1::Two, "9223372036854775808", &mut types, scope)
            .expect_err("should fail");
        assert_eq!(
            diagnostic.message(),
            "literal integer exceeds the range for a 64-bit signed integer \
             (-9223372036854775808..=9223372036854775807)"
        );

        let diagnostic = eval_v1_expr(V1::Two, "-9223372036854775809", &mut types, scope)
            .expect_err("should fail");
        assert_eq!(
            diagnostic.message(),
            "literal integer exceeds the range for a 64-bit signed integer \
             (-9223372036854775808..=9223372036854775807)"
        );
    }

    #[test]
    fn literal_float_expr() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Two, "12345.6789", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 12345.6789);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Two, "-12345.6789", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -12345.6789);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Two, "1.7976931348623157E+308", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 1.7976931348623157E+308);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Two, "-1.7976931348623157E+308", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -1.7976931348623157E+308);

        let diagnostic = eval_v1_expr(V1::Two, "2.7976931348623157E+308", &mut types, scope)
            .expect_err("should fail");
        assert_eq!(
            diagnostic.message(),
            "literal float exceeds the range for a 64-bit float \
             (-1.7976931348623157e308..=+1.7976931348623157e308)"
        );

        let diagnostic = eval_v1_expr(V1::Two, "-2.7976931348623157E+308", &mut types, scope)
            .expect_err("should fail");
        assert_eq!(
            diagnostic.message(),
            "literal float exceeds the range for a 64-bit float \
             (-1.7976931348623157e308..=+1.7976931348623157e308)"
        );
    }

    #[test]
    fn literal_string_expr() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Two, "'hello\nworld'", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "hello\nworld");

        let value = eval_v1_expr(V1::Two, r#""hello world""#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "hello world");

        let value = eval_v1_expr(
            V1::Two,
            r#"<<<
        hello  \
            world
    >>>"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "hello  world");
    }

    #[test]
    fn string_placeholders() {
        let mut root_scope = Scope::new(None);
        root_scope.insert("str", PrimitiveValue::new_string("foo"));
        root_scope.insert("file", PrimitiveValue::new_file("bar"));
        root_scope.insert("dir", PrimitiveValue::new_directory("baz"));
        root_scope.insert("salutation", PrimitiveValue::new_string("hello"));
        root_scope.insert("name1", Value::None);
        root_scope.insert("name2", PrimitiveValue::new_string("Fred"));
        root_scope.insert("spaces", PrimitiveValue::new_string("  "));
        root_scope.insert("name", PrimitiveValue::new_string("Henry"));
        root_scope.insert("company", PrimitiveValue::new_string("Acme"));

        let scopes = &[root_scope];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Two, r#""~{None}""#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "");

        let value = eval_v1_expr(V1::Two, r#""~{default="hi" None}""#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "hi");

        let value = eval_v1_expr(V1::Two, r#""~{true}""#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "true");

        let value = eval_v1_expr(V1::Two, r#""~{false}""#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "false");

        let value = eval_v1_expr(
            V1::Two,
            r#""~{true="yes" false="no" false}""#,
            &mut types,
            scope,
        )
        .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "no");

        let value = eval_v1_expr(V1::Two, r#""~{12345}""#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "12345");

        let value = eval_v1_expr(V1::Two, r#""~{12345.6789}""#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "12345.6789");

        let value = eval_v1_expr(V1::Two, r#""~{str}""#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "foo");

        let value = eval_v1_expr(V1::Two, r#""~{file}""#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "bar");

        let value = eval_v1_expr(V1::Two, r#""~{dir}""#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "baz");

        let value = eval_v1_expr(
            V1::Two,
            r#""~{sep="+" [1,2,3]} = ~{1 + 2 + 3}""#,
            &mut types,
            scope,
        )
        .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "1+2+3 = 6");

        let diagnostic =
            eval_v1_expr(V1::Two, r#""~{[1, 2, 3]}""#, &mut types, scope).expect_err("should fail");
        assert_eq!(
            diagnostic.message(),
            "cannot coerce type `Array[Int]` to `String`"
        );

        let value = eval_v1_expr(
            V1::Two,
            r#""~{salutation + ' ' + name1 + ', '}nice to meet you!""#,
            &mut types,
            scope,
        )
        .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "nice to meet you!");

        let value = eval_v1_expr(
            V1::Two,
            r#""${salutation + ' ' + name2 + ', '}nice to meet you!""#,
            &mut types,
            scope,
        )
        .unwrap();
        assert_eq!(
            value.unwrap_string().as_str(),
            "hello Fred, nice to meet you!"
        );

        let value = eval_v1_expr(
            V1::Two,
            r#"
    <<<
        ~{spaces}Hello ~{name},
        ~{spaces}Welcome to ~{company}!
    >>>"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert_eq!(
            value.unwrap_string().as_str(),
            "  Hello Henry,\n  Welcome to Acme!"
        );

        let value = eval_v1_expr(
            V1::Two,
            r#""~{1 + 2 + 3 + 4 * 10 * 10} ~{"~{<<<~{'!' + '='}>>>}"} ~{10**3}""#,
            &mut types,
            scope,
        )
        .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "406 != 1000");
    }

    #[test]
    fn literal_array_expr() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Two, "[]", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_array().to_string(), "[]");

        let value = eval_v1_expr(V1::Two, "[1, 2, 3]", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_array().to_string(), "[1, 2, 3]");

        let value = eval_v1_expr(V1::Two, "[[1], [2], [3.0]]", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_array().to_string(), "[[1.0], [2.0], [3.0]]");

        let value = eval_v1_expr(V1::Two, r#"["foo", "bar", "baz"]"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_array().to_string(), r#"["foo", "bar", "baz"]"#);
    }

    #[test]
    fn literal_pair_expr() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Two, "(true, false)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_pair().to_string(), "(true, false)");

        let value = eval_v1_expr(V1::Two, "([1, 2, 3], [4, 5, 6])", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_pair().to_string(), "([1, 2, 3], [4, 5, 6])");

        let value = eval_v1_expr(V1::Two, "([], {})", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_pair().to_string(), "([], {})");

        let value = eval_v1_expr(V1::Two, r#"("foo", "bar")"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_pair().to_string(), r#"("foo", "bar")"#);
    }

    #[test]
    fn literal_map_expr() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Two, "{}", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_map().to_string(), "{}");

        let value = eval_v1_expr(V1::Two, "{ 1: 2, 3: 4, 5: 6 }", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_map().to_string(), "{1: 2, 3: 4, 5: 6}");

        let value = eval_v1_expr(
            V1::Two,
            r#"{"foo": "bar", "baz": "qux"}"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert_eq!(
            value.unwrap_map().to_string(),
            r#"{"foo": "bar", "baz": "qux"}"#
        );

        let value = eval_v1_expr(
            V1::Two,
            r#"{"foo": { 1: 2 }, "baz": {}}"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert_eq!(
            value.unwrap_map().to_string(),
            r#"{"foo": {1: 2}, "baz": {}}"#
        );

        let value =
            eval_v1_expr(V1::Two, r#"{"foo": 100, "baz": 2.5}"#, &mut types, scope).unwrap();
        assert_eq!(
            value.unwrap_map().to_string(),
            r#"{"foo": 100.0, "baz": 2.5}"#
        );
    }

    #[test]
    fn literal_object_expr() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Two, "object {}", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_object().to_string(), "object {}");

        let value = eval_v1_expr(
            V1::Two,
            "object { foo: 2, bar: 4, baz: 6 }",
            &mut types,
            scope,
        )
        .unwrap();
        assert_eq!(
            value.unwrap_object().to_string(),
            "object {foo: 2, bar: 4, baz: 6}"
        );

        let value = eval_v1_expr(
            V1::Two,
            r#"object {foo: "bar", baz: "qux"}"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert_eq!(
            value.unwrap_object().to_string(),
            r#"object {foo: "bar", baz: "qux"}"#
        );

        let value = eval_v1_expr(
            V1::Two,
            r#"object {foo: { 1: 2 }, bar: [], qux: "jam"}"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert_eq!(
            value.unwrap_object().to_string(),
            r#"object {foo: {1: 2}, bar: [], qux: "jam"}"#
        );

        let value = eval_v1_expr(
            V1::Two,
            r#"object {foo: 1.0, bar: object { baz: "qux" }}"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert_eq!(
            value.unwrap_object().to_string(),
            r#"object {foo: 1.0, bar: object {baz: "qux"}}"#
        );
    }

    #[test]
    fn literal_struct_expr() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let bar_ty = types.add_struct(StructType::new("Bar", [
            ("foo", PrimitiveTypeKind::File),
            ("bar", PrimitiveTypeKind::Integer),
        ]));

        let foo_ty = types.add_struct(StructType::new("Foo", [
            ("foo", PrimitiveTypeKind::Float.into()),
            (
                "bar",
                Type::Compound(bar_ty.as_compound().expect("should be a compound type")),
            ),
        ]));

        let mut context =
            TestEvaluationContext::new(SupportedVersion::V1(V1::Two), &mut types, scope);
        context.structs.insert("Foo", foo_ty);
        context.structs.insert("Bar", bar_ty);

        let value = eval_v1_expr_with_context(
            r#"Foo { foo: 1.0, bar: Bar { foo: "baz", bar: 2 }}"#,
            &mut context,
        )
        .unwrap();
        assert_eq!(
            value.unwrap_struct().to_string(),
            r#"Foo {foo: 1.0, bar: Bar {foo: "baz", bar: 2}}"#
        );

        let value = eval_v1_expr_with_context(r#"Foo { foo: 1, bar: Bar { foo: "baz", bar: 2 }} == Foo { foo: 1.0, bar: Bar { foo: "baz", bar: 2 }}"#, &mut context)
            .unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr_with_context(r#"Foo { foo: 1, bar: Bar { foo: "baz", bar: 2 }} == Foo { foo: 1.0, bar: Bar { foo: "jam", bar: 2 }}"#, &mut context)
            .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr_with_context(r#"Foo { foo: 1, bar: Bar { foo: "baz", bar: 2 }} != Foo { foo: 1.0, bar: Bar { foo: "baz", bar: 2 }}"#, &mut context)
            .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr_with_context(r#"Foo { foo: 1, bar: Bar { foo: "baz", bar: 2 }} != Foo { foo: 1.0, bar: Bar { foo: "jam", bar: 2 }}"#, &mut context)
            .unwrap();
        assert!(value.unwrap_boolean());
    }

    #[test]
    fn name_ref_expr() {
        let mut root_scope = Scope::new(None);
        root_scope.insert("foo", 1234);
        let scopes = &[root_scope];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Zero, r#"foo"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 1234);

        let diagnostic = eval_v1_expr(V1::Zero, r#"bar"#, &mut types, scope).unwrap_err();
        assert_eq!(diagnostic.message(), "unknown name `bar`");
    }

    #[test]
    fn parenthesized_expr() {
        let mut root_scope = Scope::new(None);
        root_scope.insert("foo", 1234);
        let scopes = &[root_scope];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value =
            eval_v1_expr(V1::Zero, r#"(foo - foo) + (1234 - foo)"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 0);
    }

    #[test]
    fn if_expr() {
        let mut root_scope = Scope::new(None);
        root_scope.insert("foo", true);
        root_scope.insert("bar", false);
        root_scope.insert("baz", PrimitiveValue::new_file("file"));

        let scopes = &[root_scope];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(
            V1::Zero,
            r#"if (foo) then "foo" else "bar""#,
            &mut types,
            scope,
        )
        .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "foo");

        let value = eval_v1_expr(
            V1::Zero,
            r#"if (bar) then "foo" else "bar""#,
            &mut types,
            scope,
        )
        .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "bar");

        let value = eval_v1_expr(
            V1::Zero,
            r#"if (foo) then 1234 else 0.5"#,
            &mut types,
            scope,
        )
        .unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 1234.0);

        let value = eval_v1_expr(
            V1::Zero,
            r#"if (bar) then 1234 else 0.5"#,
            &mut types,
            scope,
        )
        .unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 0.5);

        let value = eval_v1_expr(
            V1::Zero,
            r#"if (foo) then baz else "str""#,
            &mut types,
            scope,
        )
        .unwrap();
        assert_eq!(value.unwrap_file().as_str(), "file");

        let value = eval_v1_expr(
            V1::Zero,
            r#"if (bar) then baz else "path""#,
            &mut types,
            scope,
        )
        .unwrap();
        assert_eq!(value.unwrap_file().as_str(), "path");
    }

    #[test]
    fn logical_not_expr() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Zero, r#"!true"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"!false"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());
    }

    #[test]
    fn negation_expr() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Zero, r#"-1234"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), -1234);

        let value = eval_v1_expr(V1::Zero, r#"-(1234)"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), -1234);

        let value = eval_v1_expr(V1::Zero, r#"----1234"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 1234);

        let value = eval_v1_expr(V1::Zero, r#"-1234.5678"#, &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -1234.5678);

        let value = eval_v1_expr(V1::Zero, r#"-(1234.5678)"#, &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -1234.5678);

        let value = eval_v1_expr(V1::Zero, r#"----1234.5678"#, &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 1234.5678);
    }

    #[test]
    fn logical_or_expr() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Zero, r#"false || false"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"false || true"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"true || false"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"true || true"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"true || nope"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let diagnostic = eval_v1_expr(V1::Zero, r#"false || nope"#, &mut types, scope).unwrap_err();
        assert_eq!(diagnostic.message(), "unknown name `nope`");
    }

    #[test]
    fn logical_and_expr() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Zero, r#"false && false"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"false && true"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"true && false"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"true && true"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"false && nope"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let diagnostic = eval_v1_expr(V1::Zero, r#"true && nope"#, &mut types, scope).unwrap_err();
        assert_eq!(diagnostic.message(), "unknown name `nope`");
    }

    #[test]
    fn equality_expr() {
        let mut root_scope = Scope::new(None);
        root_scope.insert("foo", PrimitiveValue::new_file("foo"));
        root_scope.insert("bar", PrimitiveValue::new_directory("bar"));

        let scopes = &[root_scope];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Zero, r#"None == None"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"true == true"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1234 == 1234"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1234 == 4321"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1234 == 1234.0"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"4321 == 1234.0"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1234.0 == 1234"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1234.0 == 4321"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1234.5678 == 1234.5678"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1234.5678 == 8765.4321"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#""foo" == "foo""#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#""foo" == "bar""#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#""foo" == foo"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#""foo" == bar"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"foo == "foo""#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"foo == "bar""#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"bar == "bar""#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"bar == "foo""#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(
            V1::Zero,
            r#"(1234, "bar") == (1234, "bar")"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(
            V1::Zero,
            r#"(1234, "bar") == (1234, "baz")"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"[1, 2, 3] == [1, 2, 3]"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"[1] == [2, 3]"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"[1] == [2]"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(
            V1::Zero,
            r#"{"foo": 1, "bar": 2, "baz": 3} == {"foo": 1, "bar": 2, "baz": 3}"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(
            V1::Zero,
            r#"{"foo": 1, "bar": 2, "baz": 3} == {"foo": 1, "baz": 3}"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(
            V1::Zero,
            r#"{"foo": 1, "bar": 2, "baz": 3} == {"foo": 3, "bar": 2, "baz": 1}"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(
            V1::Zero,
            r#"object {foo: 1, bar: 2, baz: "3"} == object {foo: 1, bar: 2, baz: "3"}"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(
            V1::Zero,
            r#"object {foo: 1, bar: 2, baz: "3"} == object {foo: 1, baz: "3"}"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(
            V1::Zero,
            r#"object {foo: 1, bar: 2, baz: "3"} == object {foo: 3, bar: 2, baz: "1"}"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert!(!value.unwrap_boolean());

        // Note: struct equality is handled in the struct literal test
    }

    #[test]
    fn inequality_expr() {
        let mut root_scope = Scope::new(None);
        root_scope.insert("foo", PrimitiveValue::new_file("foo"));
        root_scope.insert("bar", PrimitiveValue::new_directory("bar"));

        let scopes = &[root_scope];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Zero, r#"None != None"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"true != true"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1234 != 1234"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1234 != 4321"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1234 != 1234.0"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"4321 != 1234.0"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1234.0 != 1234"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1234.0 != 4321"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1234.5678 != 1234.5678"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1234.5678 != 8765.4321"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#""foo" != "foo""#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#""foo" != "bar""#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#""foo" != foo"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#""foo" != bar"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"foo != "foo""#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"foo != "bar""#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"bar != "bar""#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"bar != "foo""#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(
            V1::Zero,
            r#"(1234, "bar") != (1234, "bar")"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(
            V1::Zero,
            r#"(1234, "bar") != (1234, "baz")"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"[1, 2, 3] != [1, 2, 3]"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"[1] != [2, 3]"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"[1] != [2]"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(
            V1::Zero,
            r#"{"foo": 1, "bar": 2, "baz": 3} != {"foo": 1, "bar": 2, "baz": 3}"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(
            V1::Zero,
            r#"{"foo": 1, "bar": 2, "baz": 3} != {"foo": 1, "baz": 3}"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(
            V1::Zero,
            r#"{"foo": 1, "bar": 2, "baz": 3} != {"foo": 3, "bar": 2, "baz": 1}"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(
            V1::Zero,
            r#"object {foo: 1, bar: 2, baz: "3"} != object {foo: 1, bar: 2, baz: "3"}"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(
            V1::Zero,
            r#"object {foo: 1, bar: 2, baz: "3"} != object {foo: 1, baz: "3"}"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(
            V1::Zero,
            r#"object {foo: 1, bar: 2, baz: "3"} != object {foo: 3, bar: 2, baz: "1"}"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert!(value.unwrap_boolean());

        // Note: struct inequality is handled in the struct literal test
    }

    #[test]
    fn less_expr() {
        let mut root_scope = Scope::new(None);
        root_scope.insert("foo", PrimitiveValue::new_file("foo"));
        root_scope.insert("bar", PrimitiveValue::new_directory("bar"));

        let scopes = &[root_scope];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Zero, r#"false < true"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"true < false"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"true < true"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"0 < 1"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1 < 0"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1 < 1"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"0 < 1.0"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1 < 0.0"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1 < 1.0"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"0.0 < 1"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1.0 < 0"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1.0 < 1"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"0.0 < 1.0"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1.0 < 0.0"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1.0 < 1.0"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#""bar" < "foo""#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#""foo" < "bar""#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#""foo" < "foo""#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"bar < "foo""#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"bar < bar"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"foo < "bar""#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"foo < foo"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());
    }

    #[test]
    fn less_equal_expr() {
        let mut root_scope = Scope::new(None);
        root_scope.insert("foo", PrimitiveValue::new_file("foo"));
        root_scope.insert("bar", PrimitiveValue::new_directory("bar"));

        let scopes = &[root_scope];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Zero, r#"false <= true"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"true <= false"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"true <= true"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"0 <= 1"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1 <= 0"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1 <= 1"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"0 <= 1.0"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1 <= 0.0"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1 <= 1.0"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"0.0 <= 1"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1.0 <= 0"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1.0 <= 1"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"0.0 <= 1.0"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1.0 <= 0.0"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1.0 <= 1.0"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#""bar" <= "foo""#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#""foo" <= "bar""#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#""foo" <= "foo""#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"bar <= "foo""#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"bar <= bar"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"foo <= "bar""#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"foo <= foo"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());
    }

    #[test]
    fn greater_expr() {
        let mut root_scope = Scope::new(None);
        root_scope.insert("foo", PrimitiveValue::new_file("foo"));
        root_scope.insert("bar", PrimitiveValue::new_directory("bar"));

        let scopes = &[root_scope];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Zero, r#"false > true"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"true > false"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"true > true"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"0 > 1"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1 > 0"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1 > 1"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"0 > 1.0"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1 > 0.0"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1 > 1.0"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"0.0 > 1"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1.0 > 0"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1.0 > 1"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"0.0 > 1.0"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1.0 > 0.0"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1.0 > 1.0"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#""bar" > "foo""#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#""foo" > "bar""#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#""foo" > "foo""#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"bar > "foo""#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"bar > bar"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"foo > "bar""#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"foo > foo"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());
    }

    #[test]
    fn greater_equal_expr() {
        let mut root_scope = Scope::new(None);
        root_scope.insert("foo", PrimitiveValue::new_file("foo"));
        root_scope.insert("bar", PrimitiveValue::new_directory("bar"));

        let scopes = &[root_scope];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Zero, r#"false >= true"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"true >= false"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"true >= true"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"0 >= 1"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1 >= 0"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1 >= 1"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"0 >= 1.0"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1 >= 0.0"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1 >= 1.0"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"0.0 >= 1"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1.0 >= 0"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1.0 >= 1"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"0.0 >= 1.0"#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1.0 >= 0.0"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"1.0 >= 1.0"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#""bar" >= "foo""#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#""foo" >= "bar""#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#""foo" >= "foo""#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"bar >= "foo""#, &mut types, scope).unwrap();
        assert!(!value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"bar >= bar"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"foo >= "bar""#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());

        let value = eval_v1_expr(V1::Zero, r#"foo >= foo"#, &mut types, scope).unwrap();
        assert!(value.unwrap_boolean());
    }

    #[test]
    fn addition_expr() {
        let mut root_scope = Scope::new(None);
        root_scope.insert("foo", PrimitiveValue::new_file("foo"));
        root_scope.insert("bar", PrimitiveValue::new_directory("bar"));

        let scopes = &[root_scope];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Zero, r#"1 + 2 + 3 + 4"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 10);

        let value = eval_v1_expr(V1::Zero, r#"10 + 20.0 + 30 + 40.0"#, &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 100.0);

        let value =
            eval_v1_expr(V1::Zero, r#"100.0 + 200 + 300.0 + 400"#, &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 1000.0);

        let value = eval_v1_expr(
            V1::Zero,
            r#"1000.5 + 2000.5 + 3000.5 + 4000.5"#,
            &mut types,
            scope,
        )
        .unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 10002.0);

        let diagnostic = eval_v1_expr(
            V1::Zero,
            &format!(r#"{max} + 1"#, max = i64::MAX),
            &mut types,
            scope,
        )
        .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "evaluation of arithmetic expression resulted in overflow"
        );

        let value = eval_v1_expr(V1::Zero, r#""foo" + 1234"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "foo1234");

        let value = eval_v1_expr(V1::Zero, r#"1234 + "foo""#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "1234foo");

        let value = eval_v1_expr(V1::Zero, r#""foo" + 1234.456"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "foo1234.456");

        let value = eval_v1_expr(V1::Zero, r#"1234.456 + "foo""#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "1234.456foo");

        let value = eval_v1_expr(V1::Zero, r#""foo" + "bar""#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "foobar");

        let value = eval_v1_expr(V1::Zero, r#""bar" + "foo""#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "barfoo");

        let value = eval_v1_expr(V1::Zero, r#"foo + "bar""#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "foobar");

        let value = eval_v1_expr(V1::Zero, r#""bar" + foo"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "barfoo");

        let value = eval_v1_expr(V1::Zero, r#""foo" + bar"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "foobar");

        let value = eval_v1_expr(V1::Zero, r#"bar + "foo""#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "barfoo");
    }

    #[test]
    fn subtraction_expr() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Zero, r#"-1 - 2 - 3 - 4"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), -10);

        let value = eval_v1_expr(V1::Zero, r#"-10 - 20.0 - 30 - 40.0"#, &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -100.0);

        let value =
            eval_v1_expr(V1::Zero, r#"-100.0 - 200 - 300.0 - 400"#, &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -1000.0);

        let value = eval_v1_expr(
            V1::Zero,
            r#"-1000.5 - 2000.5 - 3000.5 - 4000.5"#,
            &mut types,
            scope,
        )
        .unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -10002.0);

        let diagnostic = eval_v1_expr(
            V1::Zero,
            &format!(r#"{min} - 1"#, min = i64::MIN),
            &mut types,
            scope,
        )
        .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "evaluation of arithmetic expression resulted in overflow"
        );
    }

    #[test]
    fn multiplication_expr() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Zero, r#"1 * 2 * 3 * 4"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 24);

        let value = eval_v1_expr(V1::Zero, r#"10 * 20.0 * 30 * 40.0"#, &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 240000.0);

        let value =
            eval_v1_expr(V1::Zero, r#"100.0 * 200 * 300.0 * 400"#, &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 2400000000.0);

        let value = eval_v1_expr(
            V1::Zero,
            r#"1000.5 * 2000.5 * 3000.5 * 4000.5"#,
            &mut types,
            scope,
        )
        .unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 24025008751250.063);

        let diagnostic = eval_v1_expr(
            V1::Zero,
            &format!(r#"{max} * 2"#, max = i64::MAX),
            &mut types,
            scope,
        )
        .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "evaluation of arithmetic expression resulted in overflow"
        );
    }

    #[test]
    fn division_expr() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Zero, r#"5 / 2"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 2);

        let value = eval_v1_expr(V1::Zero, r#"10 / 20.0 / 30 / 40.0"#, &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 0.00041666666666666664);

        let value =
            eval_v1_expr(V1::Zero, r#"100.0 / 200 / 300.0 / 400"#, &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 4.166666666666667e-6);

        let value = eval_v1_expr(
            V1::Zero,
            r#"1000.5 / 2000.5 / 3000.5 / 4000.5"#,
            &mut types,
            scope,
        )
        .unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 4.166492759125078e-8);

        let diagnostic = eval_v1_expr(V1::Zero, r#"10 / 0"#, &mut types, scope).unwrap_err();
        assert_eq!(diagnostic.message(), "attempt to divide by zero");

        let diagnostic = eval_v1_expr(
            V1::Zero,
            &format!(r#"{min} / -1"#, min = i64::MIN),
            &mut types,
            scope,
        )
        .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "evaluation of arithmetic expression resulted in overflow"
        );
    }

    #[test]
    fn modulo_expr() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Zero, r#"5 % 2"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 1);

        let value = eval_v1_expr(V1::Zero, r#"5.5 % 2"#, &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 1.5);

        let value = eval_v1_expr(V1::Zero, r#"5 % 2.5"#, &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 0.0);

        let value = eval_v1_expr(V1::Zero, r#"5.25 % 1.3"#, &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 0.04999999999999982);

        let diagnostic = eval_v1_expr(V1::Zero, r#"5 % 0"#, &mut types, scope).unwrap_err();
        assert_eq!(diagnostic.message(), "attempt to divide by zero");

        let diagnostic = eval_v1_expr(
            V1::Zero,
            &format!(r#"{min} % -1"#, min = i64::MIN),
            &mut types,
            scope,
        )
        .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "evaluation of arithmetic expression resulted in overflow"
        );
    }

    #[test]
    fn exponentiation_expr() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let diagnostic = eval_v1_expr(V1::Zero, r#"10 ** 0"#, &mut types, scope).unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "use of the exponentiation operator requires WDL version 1.2"
        );

        let value = eval_v1_expr(V1::Two, r#"5 ** 2 ** 2"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 625);

        let value = eval_v1_expr(V1::Two, r#"5 ** 2.0 ** 2"#, &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 625.0);

        let value = eval_v1_expr(V1::Two, r#"5 ** 2 ** 2.0"#, &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 625.0);

        let value = eval_v1_expr(V1::Two, r#"5.0 ** 2.0 ** 2.0"#, &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 625.0);

        let diagnostic = eval_v1_expr(
            V1::Two,
            &format!(r#"{max} ** 2"#, max = i64::MAX),
            &mut types,
            scope,
        )
        .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "evaluation of arithmetic expression resulted in overflow"
        );
    }

    #[test]
    fn index_expr() {
        let mut types = Types::default();
        let array_ty = types.add_array(ArrayType::new(PrimitiveTypeKind::Integer));
        let map_ty = types.add_map(MapType::new(
            PrimitiveTypeKind::String,
            PrimitiveTypeKind::Integer,
        ));

        let mut root_scope = Scope::new(None);
        root_scope.insert(
            "foo",
            Array::new(&types, array_ty, [1, 2, 3, 4, 5]).unwrap(),
        );
        root_scope.insert(
            "bar",
            Map::new(&types, map_ty, [
                (PrimitiveValue::new_string("foo"), 1),
                (PrimitiveValue::new_string("bar"), 2),
            ])
            .unwrap(),
        );
        root_scope.insert("baz", PrimitiveValue::new_file("bar"));

        let scopes = &[root_scope];
        let scope = ScopeRef::new(scopes, 0);

        let value = eval_v1_expr(V1::Zero, r#"foo[1]"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 2);

        let value = eval_v1_expr(V1::Zero, r#"foo[foo[[1, 2, 3][0]]]"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 3);

        let diagnostic = eval_v1_expr(V1::Zero, r#"foo[10]"#, &mut types, scope).unwrap_err();
        assert_eq!(diagnostic.message(), "array index 10 is out of range");

        let diagnostic = eval_v1_expr(V1::Zero, r#"foo["10"]"#, &mut types, scope).unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "type mismatch: expected index to be type `Int`, but found type `String`"
        );

        let value = eval_v1_expr(V1::Zero, r#"bar["foo"]"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 1);

        let value = eval_v1_expr(V1::Zero, r#"bar[baz]"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 2);

        let value = eval_v1_expr(V1::Zero, r#"foo[bar["foo"]]"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 2);

        let diagnostic =
            eval_v1_expr(V1::Zero, r#"bar["does not exist"]"#, &mut types, scope).unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "the map does not contain an entry for the specified key"
        );

        let diagnostic = eval_v1_expr(V1::Zero, r#"bar[1]"#, &mut types, scope).unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "type mismatch: expected index to be type `String`, but found type `Int`"
        );

        let diagnostic = eval_v1_expr(V1::Zero, r#"1[0]"#, &mut types, scope).unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "indexing is only allowed on `Array` and `Map` types"
        );
    }

    #[test]
    fn access_expr() {
        let mut types = Types::default();
        let pair_ty = types.add_pair(PairType::new(
            PrimitiveTypeKind::Integer,
            PrimitiveTypeKind::String,
        ));
        let struct_ty = types.add_struct(StructType::new("Foo", [
            ("foo", PrimitiveTypeKind::Integer),
            ("bar", PrimitiveTypeKind::String),
        ]));

        let mut root_scope = Scope::new(None);
        root_scope.insert(
            "foo",
            Pair::new(&types, pair_ty, 1, PrimitiveValue::new_string("foo")).unwrap(),
        );
        root_scope.insert(
            "bar",
            Struct::new(&types, struct_ty, [
                ("foo", 1.into()),
                ("bar", PrimitiveValue::new_string("bar")),
            ])
            .unwrap(),
        );
        root_scope.insert("baz", 1);

        let scopes = &[root_scope];
        let scope = ScopeRef::new(scopes, 0);

        let value = eval_v1_expr(V1::Zero, r#"foo.left"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 1);

        let value = eval_v1_expr(V1::Zero, r#"foo.right"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "foo");

        let diagnostic = eval_v1_expr(V1::Zero, r#"foo.bar"#, &mut types, scope).unwrap_err();
        assert_eq!(diagnostic.message(), "cannot access a pair with name `bar`");

        let value = eval_v1_expr(V1::Zero, r#"bar.foo"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 1);

        let value = eval_v1_expr(V1::Zero, r#"bar.bar"#, &mut types, scope).unwrap();
        assert_eq!(value.unwrap_string().as_str(), "bar");

        let diagnostic = eval_v1_expr(V1::Zero, r#"bar.baz"#, &mut types, scope).unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "struct `Foo` does not have a member named `baz`"
        );

        let value = eval_v1_expr(
            V1::Zero,
            r#"object { foo: 1, bar: "bar" }.foo"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert_eq!(value.unwrap_integer(), 1);

        let value = eval_v1_expr(
            V1::Zero,
            r#"object { foo: 1, bar: "bar" }.bar"#,
            &mut types,
            scope,
        )
        .unwrap();
        assert_eq!(value.unwrap_string().as_str(), "bar");

        let diagnostic = eval_v1_expr(
            V1::Zero,
            r#"object { foo: 1, bar: "bar" }.baz"#,
            &mut types,
            scope,
        )
        .unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "object does not have a member named `baz`"
        );

        let diagnostic = eval_v1_expr(V1::Zero, r#"baz.foo"#, &mut types, scope).unwrap_err();
        assert_eq!(diagnostic.message(), "cannot access type `Int`");
    }
}

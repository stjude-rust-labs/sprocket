//! Module for the WDL standard library implementation.

use std::collections::HashMap;
use std::sync::LazyLock;

use wdl_analysis::stdlib::Binding;
use wdl_analysis::types::PrimitiveTypeKind;
use wdl_analysis::types::Type;
use wdl_analysis::types::TypeEq;
use wdl_analysis::types::Types;
use wdl_ast::Diagnostic;

use crate::Coercible;
use crate::Value;

/// Rounds a floating point number down to the next lower integer.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#floor
pub fn floor(types: &Types, arguments: &[Value], return_type: Type) -> Result<Value, Diagnostic> {
    debug_assert_eq!(arguments.len(), 1);
    debug_assert!(return_type.type_eq(types, &PrimitiveTypeKind::Integer.into()));

    let arg = arguments[0]
        .coerce(types, PrimitiveTypeKind::Float.into())
        .expect("value should coerce to float")
        .unwrap_float();
    Ok((arg.floor() as i64).into())
}

/// Rounds a floating point number up to the next higher integer.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#ceil
pub fn ceil(types: &Types, arguments: &[Value], return_type: Type) -> Result<Value, Diagnostic> {
    debug_assert_eq!(arguments.len(), 1);
    debug_assert!(return_type.type_eq(types, &PrimitiveTypeKind::Integer.into()));

    let arg = arguments[0]
        .coerce(types, PrimitiveTypeKind::Float.into())
        .expect("value should coerce to float")
        .unwrap_float();
    Ok((arg.ceil() as i64).into())
}

/// Rounds a floating point number to the nearest integer based on standard
/// rounding rules ("round half up").
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#round
pub fn round(types: &Types, arguments: &[Value], return_type: Type) -> Result<Value, Diagnostic> {
    debug_assert_eq!(arguments.len(), 1);
    debug_assert!(return_type.type_eq(types, &PrimitiveTypeKind::Integer.into()));

    let arg = arguments[0]
        .coerce(types, PrimitiveTypeKind::Float.into())
        .expect("value should coerce to float")
        .unwrap_float();
    Ok((arg.round() as i64).into())
}

/// Returns the smaller of two integer values.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#min
pub fn int_min(types: &Types, arguments: &[Value], return_type: Type) -> Result<Value, Diagnostic> {
    debug_assert_eq!(arguments.len(), 2);
    debug_assert!(return_type.type_eq(types, &PrimitiveTypeKind::Integer.into()));

    let first = arguments[0]
        .coerce(types, PrimitiveTypeKind::Integer.into())
        .expect("value should coerce to integer")
        .unwrap_integer();
    let second = arguments[1]
        .coerce(types, PrimitiveTypeKind::Integer.into())
        .expect("value should coerce to integer")
        .unwrap_integer();
    Ok(first.min(second).into())
}

/// Returns the smaller of two float values.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#min
pub fn float_min(
    types: &Types,
    arguments: &[Value],
    return_type: Type,
) -> Result<Value, Diagnostic> {
    debug_assert_eq!(arguments.len(), 2);
    debug_assert!(return_type.type_eq(types, &PrimitiveTypeKind::Float.into()));

    let first = arguments[0]
        .coerce(types, PrimitiveTypeKind::Float.into())
        .expect("value should coerce to float")
        .unwrap_float();
    let second = arguments[1]
        .coerce(types, PrimitiveTypeKind::Float.into())
        .expect("value should coerce to float")
        .unwrap_float();
    Ok(first.min(second).into())
}

/// Returns the larger of two integer values.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#max
pub fn int_max(types: &Types, arguments: &[Value], return_type: Type) -> Result<Value, Diagnostic> {
    assert_eq!(arguments.len(), 2);
    debug_assert!(return_type.type_eq(types, &PrimitiveTypeKind::Integer.into()));

    let first = arguments[0]
        .coerce(types, PrimitiveTypeKind::Integer.into())
        .expect("value should coerce to integer")
        .unwrap_integer();
    let second = arguments[1]
        .coerce(types, PrimitiveTypeKind::Integer.into())
        .expect("value should coerce to integer")
        .unwrap_integer();
    Ok(first.max(second).into())
}

/// Returns the larger of two float values.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#max
pub fn float_max(
    types: &Types,
    arguments: &[Value],
    return_type: Type,
) -> Result<Value, Diagnostic> {
    assert_eq!(arguments.len(), 2);
    debug_assert!(return_type.type_eq(types, &PrimitiveTypeKind::Float.into()));

    let first = arguments[0]
        .coerce(types, PrimitiveTypeKind::Float.into())
        .expect("value should coerce to float")
        .unwrap_float();
    let second = arguments[1]
        .coerce(types, PrimitiveTypeKind::Float.into())
        .expect("value should coerce to float")
        .unwrap_float();
    Ok(first.max(second).into())
}

/// Represents a WDL function implementation callback.
type Callback = fn(&Types, &[Value], Type) -> Result<Value, Diagnostic>;

/// Represents an implementation signature for a WDL standard library function.
#[derive(Debug, Clone, Copy)]
pub struct Signature {
    /// The display string of the signature.
    ///
    /// This is only used for unit tests.
    #[allow(unused)]
    display: &'static str,
    /// The implementation callback of the signature.
    callback: Callback,
}

impl Signature {
    /// Constructs a new signature given its display and callback.
    const fn new(display: &'static str, callback: Callback) -> Self {
        Self { display, callback }
    }
}

/// Represents a standard library function.
#[derive(Debug, Clone, Copy)]
pub struct Function {
    /// The signatures of the function.
    signatures: &'static [Signature],
}

impl Function {
    /// Constructs a new function given its signatures.
    const fn new(signatures: &'static [Signature]) -> Self {
        Self { signatures }
    }

    /// Calls the function given the binding and the function's arguments.
    #[inline]
    pub fn call(
        &self,
        binding: Binding<'_>,
        types: &Types,
        arguments: &[Value],
    ) -> Result<Value, Diagnostic> {
        (self.signatures[binding.index()].callback)(types, arguments, binding.return_type())
    }
}

/// Represents the WDL standard library.
#[derive(Debug)]
pub struct StandardLibrary {
    /// The implementation functions for the standard library.
    functions: HashMap<&'static str, Function>,
}

impl StandardLibrary {
    /// Gets a function from the standard library.
    ///
    /// Returns `None` if the function isn't in the WDL standard library.
    #[inline]
    pub fn get(&self, name: &str) -> Option<Function> {
        self.functions.get(name).copied()
    }
}

/// Represents the mapping between function name and overload index to the
/// implementation callback.
pub static STDLIB: LazyLock<StandardLibrary> = LazyLock::new(|| {
    let mut functions = HashMap::with_capacity(wdl_analysis::stdlib::STDLIB.functions().len());
    assert!(
        functions
            .insert(
                "floor",
                Function::new(const { &[Signature::new("(Float) -> Int", floor)] })
            )
            .is_none()
    );
    assert!(
        functions
            .insert(
                "ceil",
                Function::new(const { &[Signature::new("(Float) -> Int", ceil)] })
            )
            .is_none()
    );
    assert!(
        functions
            .insert(
                "round",
                Function::new(const { &[Signature::new("(Float) -> Int", round)] })
            )
            .is_none()
    );
    assert!(
        functions
            .insert(
                "min",
                Function::new(
                    const {
                        &[
                            Signature::new("(Int, Int) -> Int", int_min),
                            Signature::new("(Int, Float) -> Float", float_min),
                            Signature::new("(Float, Int) -> Float", float_min),
                            Signature::new("(Float, Float) -> Float", float_min),
                        ]
                    }
                )
            )
            .is_none()
    );
    assert!(
        functions
            .insert(
                "max",
                Function::new(
                    const {
                        &[
                            Signature::new("(Int, Int) -> Int", int_max),
                            Signature::new("(Int, Float) -> Float", float_max),
                            Signature::new("(Float, Int) -> Float", float_max),
                            Signature::new("(Float, Float) -> Float", float_max),
                        ]
                    }
                )
            )
            .is_none()
    );

    StandardLibrary { functions }
});

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_analysis::stdlib::TypeParameters;
    use wdl_ast::version::V1;

    use super::*;
    use crate::Scope;
    use crate::ScopeRef;
    use crate::v1::test::eval_v1_expr;

    /// A test to verify that the STDLIB function types from `wdl-analysis`
    /// aligns with the STDLIB implementation from `wdl-engine`.
    #[test]
    fn verify_stdlib() {
        for (name, func) in wdl_analysis::stdlib::STDLIB.functions() {
            match STDLIB.functions.get(name) {
                Some(imp) => match func {
                    wdl_analysis::stdlib::Function::Monomorphic(f) => {
                        assert_eq!(imp.signatures.len(), 1);
                        assert_eq!(
                            f.signature()
                                .display(
                                    wdl_analysis::stdlib::STDLIB.types(),
                                    &TypeParameters::new(f.signature().type_parameters())
                                )
                                .to_string(),
                            imp.signatures[0].display
                        );
                    }
                    wdl_analysis::stdlib::Function::Polymorphic(f) => {
                        assert_eq!(imp.signatures.len(), f.signatures().len());
                        for (i, sig) in f.signatures().iter().enumerate() {
                            assert_eq!(
                                sig.display(
                                    wdl_analysis::stdlib::STDLIB.types(),
                                    &TypeParameters::new(sig.type_parameters())
                                )
                                .to_string(),
                                imp.signatures[i].display
                            );
                        }
                    }
                },
                None => {
                    // TODO: make this a failure in the future once the entire STDLIB is implemented
                    continue;
                }
            }
        }
    }

    #[test]
    fn floor() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Zero, "floor(10.5)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 10);

        let value = eval_v1_expr(V1::Zero, "floor(10)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 10);

        let value = eval_v1_expr(V1::Zero, "floor(9.9999)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 9);

        let value = eval_v1_expr(V1::Zero, "floor(0)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 0);

        let value = eval_v1_expr(V1::Zero, "floor(-5.1)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), -6);
    }

    #[test]
    fn ceil() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Zero, "ceil(10.5)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 11);

        let value = eval_v1_expr(V1::Zero, "ceil(10)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 10);

        let value = eval_v1_expr(V1::Zero, "ceil(9.9999)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 10);

        let value = eval_v1_expr(V1::Zero, "ceil(0)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 0);

        let value = eval_v1_expr(V1::Zero, "ceil(-5.1)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), -5);
    }

    #[test]
    fn round() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::Zero, "round(10.5)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 11);

        let value = eval_v1_expr(V1::Zero, "round(10.3)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 10);

        let value = eval_v1_expr(V1::Zero, "round(10)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 10);

        let value = eval_v1_expr(V1::Zero, "round(9.9999)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 10);

        let value = eval_v1_expr(V1::Zero, "round(9.12345)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 9);

        let value = eval_v1_expr(V1::Zero, "round(0)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 0);

        let value = eval_v1_expr(V1::Zero, "round(-5.1)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), -5);

        let value = eval_v1_expr(V1::Zero, "round(-5.5)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), -6);
    }

    #[test]
    fn min() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::One, "min(7, 42)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 7);

        let value = eval_v1_expr(V1::One, "min(42, 7)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 7);

        let value = eval_v1_expr(V1::One, "min(-42, 7)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), -42);

        let value = eval_v1_expr(V1::One, "min(0, -42)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), -42);

        let value = eval_v1_expr(V1::One, "min(0, 42)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 0);

        let value = eval_v1_expr(V1::One, "min(7.0, 42)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 7.0);

        let value = eval_v1_expr(V1::One, "min(42.0, 7)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 7.0);

        let value = eval_v1_expr(V1::One, "min(-42.0, 7)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -42.0);

        let value = eval_v1_expr(V1::One, "min(0.0, -42)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -42.0);

        let value = eval_v1_expr(V1::One, "min(0.0, 42)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -0.0);

        let value = eval_v1_expr(V1::One, "min(7, 42.0)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 7.0);

        let value = eval_v1_expr(V1::One, "min(42, 7.0)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 7.0);

        let value = eval_v1_expr(V1::One, "min(-42, 7.0)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -42.0);

        let value = eval_v1_expr(V1::One, "min(0, -42.0)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -42.0);

        let value = eval_v1_expr(V1::One, "min(0, 42.0)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -0.0);

        let value = eval_v1_expr(V1::One, "min(7.0, 42.0)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 7.0);

        let value = eval_v1_expr(V1::One, "min(42.0, 7.0)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 7.0);

        let value = eval_v1_expr(V1::One, "min(-42.0, 7.0)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -42.0);

        let value = eval_v1_expr(V1::One, "min(0.0, -42.0)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -42.0);

        let value = eval_v1_expr(V1::One, "min(0.0, 42.0)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -0.0);

        let value = eval_v1_expr(
            V1::One,
            "min(12345, min(-100, min(54321, 1234.5678)))",
            &mut types,
            scope,
        )
        .unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), -100.0);
    }

    #[test]
    fn max() {
        let scopes = &[Scope::new(None)];
        let scope = ScopeRef::new(scopes, 0);

        let mut types = Types::default();
        let value = eval_v1_expr(V1::One, "max(7, 42)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 42);

        let value = eval_v1_expr(V1::One, "max(42, 7)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 42);

        let value = eval_v1_expr(V1::One, "max(-42, 7)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 7);

        let value = eval_v1_expr(V1::One, "max(0, -42)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 0);

        let value = eval_v1_expr(V1::One, "max(0, 42)", &mut types, scope).unwrap();
        assert_eq!(value.unwrap_integer(), 42);

        let value = eval_v1_expr(V1::One, "max(7.0, 42)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 42.0);

        let value = eval_v1_expr(V1::One, "max(42.0, 7)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 42.0);

        let value = eval_v1_expr(V1::One, "max(-42.0, 7)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 7.0);

        let value = eval_v1_expr(V1::One, "max(0.0, -42)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 0.0);

        let value = eval_v1_expr(V1::One, "max(0.0, 42)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 42.0);

        let value = eval_v1_expr(V1::One, "max(7, 42.0)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 42.0);

        let value = eval_v1_expr(V1::One, "max(42, 7.0)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 42.0);

        let value = eval_v1_expr(V1::One, "max(-42, 7.0)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 7.0);

        let value = eval_v1_expr(V1::One, "max(0, -42.0)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 0.0);

        let value = eval_v1_expr(V1::One, "max(0, 42.0)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 42.0);

        let value = eval_v1_expr(V1::One, "max(7.0, 42.0)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 42.0);

        let value = eval_v1_expr(V1::One, "max(42.0, 7.0)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 42.0);

        let value = eval_v1_expr(V1::One, "max(-42.0, 7.0)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 7.0);

        let value = eval_v1_expr(V1::One, "max(0.0, -42.0)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 0.0);

        let value = eval_v1_expr(V1::One, "max(0.0, 42.0)", &mut types, scope).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 42.0);

        let value = eval_v1_expr(
            V1::One,
            "max(12345, max(-100, max(54321, 1234.5678)))",
            &mut types,
            scope,
        )
        .unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 54321.0);
    }
}

//! Implements the `length` function from the WDL standard library.

use wdl_analysis::types::PrimitiveTypeKind;
use wdl_analysis::types::Type;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// Returns the length of the input argument as an Int:
///
/// For an `Array[X]` argument: the number of elements in the array.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#length
fn array_length(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 1);
    debug_assert!(context.return_type_eq(PrimitiveTypeKind::Integer));
    Ok(i64::try_from(
        context.arguments[0]
            .value
            .as_array()
            .expect("argument should be an array")
            .len(),
    )
    .map_err(|_| {
        function_call_failed(
            "length",
            "array length exceeds a signed 64-bit integer",
            context.call_site,
        )
    })?
    .into())
}

/// Returns the length of the input argument as an Int:
///
/// For a `Map[X, Y]` argument: the number of items in the map.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#length
fn map_length(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 1);
    debug_assert!(context.return_type_eq(PrimitiveTypeKind::Integer));
    Ok(i64::try_from(
        context.arguments[0]
            .value
            .as_map()
            .expect("argument should be a map")
            .len(),
    )
    .map_err(|_| {
        function_call_failed(
            "length",
            "map length exceeds a signed 64-bit integer",
            context.call_site,
        )
    })?
    .into())
}

/// Returns the length of the input argument as an Int:
///
/// For an `Object` argument: the number of key-value pairs in the object.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#length
fn object_length(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 1);
    debug_assert!(context.return_type_eq(PrimitiveTypeKind::Integer));
    let object = context.coerce_argument(0, Type::Object).unwrap_object();

    Ok(i64::try_from(object.len())
        .map_err(|_| {
            function_call_failed(
                "length",
                "object members length exceeds a signed 64-bit integer",
                context.call_site,
            )
        })?
        .into())
}

/// Returns the length of the input argument as an Int:
///
/// For a `String` argument: the number of characters in the string.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#length
fn string_length(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 1);
    debug_assert!(context.return_type_eq(PrimitiveTypeKind::Integer));
    let s = context
        .coerce_argument(0, PrimitiveTypeKind::String)
        .unwrap_string();

    // Note: the function is defined in terms of characters and not bytes
    // This is a O(N) operation
    Ok(i64::try_from(s.chars().count())
        .map_err(|_| {
            function_call_failed(
                "length",
                "string character length exceeds a signed 64-bit integer",
                context.call_site,
            )
        })?
        .into())
}

/// Gets the function describing `length`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[
                Signature::new("(Array[X]) -> Int", array_length),
                Signature::new("(Map[K, V]) -> Int", map_length),
                Signature::new("(Object) -> Int", object_length),
                Signature::new("(String) -> Int", string_length),
            ]
        },
    )
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn length() {
        let mut env = TestEnv::default();

        let value = eval_v1_expr(&mut env, V1::Zero, "length([])").unwrap();
        assert_eq!(value.unwrap_integer(), 0);

        let value = eval_v1_expr(&mut env, V1::Zero, "length({})").unwrap();
        assert_eq!(value.unwrap_integer(), 0);

        let value = eval_v1_expr(&mut env, V1::Zero, "length(object {})").unwrap();
        assert_eq!(value.unwrap_integer(), 0);

        let value = eval_v1_expr(&mut env, V1::Zero, "length('')").unwrap();
        assert_eq!(value.unwrap_integer(), 0);

        let value = eval_v1_expr(&mut env, V1::Zero, "length([1, 2, 3, 4, 5])").unwrap();
        assert_eq!(value.unwrap_integer(), 5);

        let value = eval_v1_expr(
            &mut env,
            V1::Zero,
            "length({ 'foo': 1, 'bar': 2, 'baz': 3})",
        )
        .unwrap();
        assert_eq!(value.unwrap_integer(), 3);

        let value = eval_v1_expr(
            &mut env,
            V1::Zero,
            "length(object { foo: 1, bar: 2, baz: 3})",
        )
        .unwrap();
        assert_eq!(value.unwrap_integer(), 3);

        let value = eval_v1_expr(&mut env, V1::Zero, "length('hello world!')").unwrap();
        assert_eq!(value.unwrap_integer(), 12);
    }
}

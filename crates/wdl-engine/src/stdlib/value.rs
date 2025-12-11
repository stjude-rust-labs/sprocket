//! Implements the `value` function from the WDL standard library.

use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Value;
use crate::diagnostics::function_call_failed;

/// The name of the standard library function.
const FUNCTION_NAME: &str = "value";

/// Returns the underlying value associated with an enum variant.
fn value(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 1);

    let variant = context.arguments[0]
        .value
        .as_compound()
        .and_then(|c| c.as_enum_variant())
        .ok_or_else(|| {
            function_call_failed(FUNCTION_NAME, "expected an enum value", context.call_site)
        })?;

    Ok(variant.value().clone())
}

/// Gets the function describing `value`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(variant: V) -> T where `V`: any enumeration variant",
                Callback::Sync(value),
            )]
        },
    )
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use wdl_analysis::types::EnumType;
    use wdl_analysis::types::PrimitiveType;
    use wdl_ast::Span;
    use wdl_ast::version::V1;

    use crate::CompoundValue;
    use crate::PrimitiveValue;
    use crate::Value;
    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;
    use crate::value::EnumVariant;

    #[tokio::test]
    async fn value() {
        let mut env = TestEnv::default();

        let enum_ty = EnumType::new(
            "Color",
            Span::new(0, 0),
            PrimitiveType::String.into(),
            vec![
                ("Red".into(), PrimitiveType::String.into()),
                ("Green".into(), PrimitiveType::String.into()),
                ("Blue".into(), PrimitiveType::String.into()),
            ],
            &[Span::new(0, 0), Span::new(0, 0), Span::new(0, 0)],
        )
        .unwrap();
        env.insert_enum("Color", enum_ty.clone());

        let color_red = EnumVariant::new(
            enum_ty.clone(),
            "Red",
            PrimitiveValue::String(Arc::new(String::from("#FF0000"))),
        );

        env.insert_name(
            "color_red",
            Value::Compound(CompoundValue::EnumVariant(color_red)),
        );

        let result = eval_v1_expr(&env, V1::Three, "value(color_red)")
            .await
            .unwrap();
        assert_eq!(result.unwrap_string().as_str(), "#FF0000");

        let int_enum = EnumType::new(
            "Status",
            Span::new(0, 0),
            PrimitiveType::Integer.into(),
            vec![
                ("Active".into(), PrimitiveType::Integer.into()),
                ("Inactive".into(), PrimitiveType::Integer.into()),
            ],
            &[Span::new(0, 0), Span::new(0, 0)],
        )
        .unwrap();

        let status_active =
            EnumVariant::new(int_enum.clone(), "Active", PrimitiveValue::Integer(1));
        let status_inactive =
            EnumVariant::new(int_enum.clone(), "Inactive", PrimitiveValue::Integer(42));

        env.insert_name(
            "status_active",
            Value::Compound(CompoundValue::EnumVariant(status_active)),
        );
        env.insert_name(
            "status_inactive",
            Value::Compound(CompoundValue::EnumVariant(status_inactive)),
        );

        let result = eval_v1_expr(&env, V1::Three, "value(status_active)")
            .await
            .unwrap();
        assert_eq!(result.unwrap_integer(), 1);

        let result = eval_v1_expr(&env, V1::Three, "value(status_inactive)")
            .await
            .unwrap();
        assert_eq!(result.unwrap_integer(), 42);
    }

    #[tokio::test]
    async fn value_non_enum() {
        let mut env = TestEnv::default();
        env.insert_name("s", Value::Primitive(PrimitiveValue::new_string("hello")));

        let diagnostic = eval_v1_expr(&env, V1::Three, "value(s)").await.unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "type mismatch: argument to function `value` expects type `V` where `V`: any \
             enumeration variant, but found type `String`"
        );
    }
}

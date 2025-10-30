//! Implements the `value` function from the WDL standard library.

use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Value;
use crate::diagnostics::function_call_failed;

const FUNCTION_NAME: &str = "value";

/// Returns the underlying value associated with an enum variant.
fn value(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 1);

    let enum_value = context.arguments[0]
        .value
        .as_compound()
        .and_then(|c| c.as_enum())
        .ok_or_else(|| {
            function_call_failed(
                FUNCTION_NAME,
                "expected an enum value",
                context.call_site,
            )
        })?;

    Ok((*enum_value.value()).clone())
}

/// Gets the function describing `value`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(variant: Enum[T]) -> T",
                Callback::Sync(value),
            )]
        },
    )
}

#[cfg(test)]
mod test {
    use wdl_analysis::types::CompoundType;
    use wdl_analysis::types::EnumType;
    use wdl_analysis::types::PrimitiveType;
    use wdl_analysis::types::Type;
    use wdl_ast::version::V1;

    use crate::PrimitiveValue;
    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;
    use crate::value::Enum;
    use crate::CompoundValue;
    use crate::Value;

    #[tokio::test]
    async fn value() {
        let mut env = TestEnv::default();

        let enum_ty = EnumType::new(
            "Color",
            PrimitiveType::String,
            [
                ("Red", PrimitiveType::String),
                ("Green", PrimitiveType::String),
                ("Blue", PrimitiveType::String),
            ],
        )
        .unwrap();

        let color_red = Enum::new(
            None,
            Type::Compound(CompoundType::Enum(enum_ty.clone().into()), false),
            "Red",
            PrimitiveValue::new_string("#FF0000"),
        )
        .unwrap();

        env.insert_name("color_red", Value::Compound(CompoundValue::Enum(color_red)));

        let result = eval_v1_expr(&env, V1::Three, "value(color_red)")
            .await
            .unwrap();
        assert_eq!(result.unwrap_string().as_str(), "#FF0000");

        let int_enum = EnumType::new(
            "Status",
            PrimitiveType::Integer,
            [
                ("Active", PrimitiveType::Integer),
                ("Inactive", PrimitiveType::Integer),
            ],
        )
        .unwrap();

        let status_active = Enum::new(
            None,
            Type::Compound(CompoundType::Enum(int_enum.clone().into()), false),
            "Active",
            PrimitiveValue::Integer(1),
        )
        .unwrap();

        let status_inactive = Enum::new(
            None,
            Type::Compound(CompoundType::Enum(int_enum.clone().into()), false),
            "Inactive",
            PrimitiveValue::Integer(42),
        )
        .unwrap();

        env.insert_name("status_active", Value::Compound(CompoundValue::Enum(status_active)));
        env.insert_name(
            "status_inactive",
            Value::Compound(CompoundValue::Enum(status_inactive)),
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
}

//! Implements the `variants` function from the WDL standard library.

use wdl_analysis::types::CompoundType;
use wdl_analysis::types::Type;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Callback;
use super::Function;
use super::Signature;
use crate::Value;
use crate::diagnostics::function_call_failed;

const FUNCTION_NAME: &str = "variants";

/// Returns an array of all variants for an enum type.
fn variants(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert_eq!(context.arguments.len(), 1);

    let arg = &context.arguments[0].value;

    if let Type::Compound(CompoundType::Enum(enum_ty), _) = arg.ty() {
        if let Some(variants_array) = context.inner().get_enum_variants(enum_ty.name()) {
            return Ok(Value::Compound(crate::CompoundValue::Array(
                (*variants_array).clone(),
            )));
        }

        return Err(function_call_failed(
            FUNCTION_NAME,
            format!("unknown enum `{}`", enum_ty.name()),
            context.call_site,
        ));
    }

    Err(function_call_failed(
        FUNCTION_NAME,
        "expected an enum type argument",
        context.call_site,
    ))
}

/// Gets the function describing `variants`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[Signature::new(
                "(enum_type: Enum[T]) -> Array[Enum[T]]",
                Callback::Sync(variants),
            )]
        },
    )
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use wdl_analysis::types::CompoundType;
    use wdl_analysis::types::EnumType;
    use wdl_analysis::types::PrimitiveType;
    use wdl_analysis::types::Type;
    use wdl_ast::version::V1;

    use crate::Array;
    use crate::CompoundValue;
    use crate::PrimitiveValue;
    use crate::Value;
    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;
    use crate::value::Enum;

    #[tokio::test]
    async fn variants() {
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

        let enum_type = Type::Compound(CompoundType::Enum(enum_ty.clone().into()), false);

        // Construct the variant values
        let red = Enum::new(None, enum_type.clone(), "Red", PrimitiveValue::new_string("#FF0000")).unwrap();
        let green =
            Enum::new(None, enum_type.clone(), "Green", PrimitiveValue::new_string("#00FF00")).unwrap();
        let blue =
            Enum::new(None, enum_type.clone(), "Blue", PrimitiveValue::new_string("#0000FF")).unwrap();

        // Create Array[Color] type
        use wdl_analysis::types::ArrayType;
        let array_type = Type::Compound(
            CompoundType::Array(ArrayType::new(enum_type.clone()).into()),
            false,
        );

        let variants_array = Arc::new(
            Array::new(
                None,
                array_type,
                vec![
                    Value::Compound(CompoundValue::Enum(red)),
                    Value::Compound(CompoundValue::Enum(green)),
                    Value::Compound(CompoundValue::Enum(blue)),
                ],
            )
            .unwrap(),
        );

        env.insert_enum("Color", enum_type, variants_array.clone());

        // For now, we expect an error because Color evaluates as a name, not a type
        let result = eval_v1_expr(&env, V1::Three, "variants(Color)").await;
        assert!(result.is_err());

        // But if we pass an actual enum value, it should work
        let red_value = Enum::new(
            None,
            Type::Compound(CompoundType::Enum(enum_ty.into()), false),
            "Red",
            PrimitiveValue::new_string("#FF0000"),
        )
        .unwrap();
        env.insert_name("color_red", Value::Compound(CompoundValue::Enum(red_value)));

        let result = eval_v1_expr(&env, V1::Three, "variants(color_red)")
            .await
            .unwrap();

        let result_array = result.as_compound().unwrap().as_array().unwrap();
        assert_eq!(result_array.len(), 3);

        let variant_names: Vec<_> = result_array
            .as_slice()
            .iter()
            .map(|v| {
                v.as_compound()
                    .unwrap()
                    .as_enum()
                    .unwrap()
                    .variant_name()
                    .to_string()
            })
            .collect();

        assert_eq!(variant_names, vec!["Red", "Green", "Blue"]);
    }
}

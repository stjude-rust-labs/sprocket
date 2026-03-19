version 1.0

workflow inline_if_then_else_long {
    input {
        Boolean very_long_condition_name
        Int something_big_expression_value
        Int something_else_expression_value
    }

    Int y = if very_long_condition_name
        then something_big_expression_value
        else something_else_expression_value
}

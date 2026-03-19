version 1.0

workflow inline_if_then_else_disabled {
    input {
        Boolean x
        Int a
        Int b
    }

    Int y = if x then a else b
}

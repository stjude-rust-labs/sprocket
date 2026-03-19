version 1.0

workflow inline_if_then_else_short {
    input {
        Boolean x
        Int a
        Int b
    }

    Int y = if x
        then a
        else b
}

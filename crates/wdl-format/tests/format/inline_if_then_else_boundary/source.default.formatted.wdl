version 1.0

workflow inline_if_then_else_boundary {
    input {
        Boolean x
        Int aa
        Int bb
    }

    Int y = if x
        then aa
        else bb
}

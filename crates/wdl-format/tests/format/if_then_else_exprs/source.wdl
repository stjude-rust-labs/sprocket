## This is a test WDL file for if-then-else expressions
version 1.0
workflow if_then_else_exprs {
    input {
        Int a
        Int b
    }

    Int c = (
        if (a < b)
        then a
        else b
    )

    Int d =
        if (a < b)
        then a
        else b

    output {
        Int result = c
    }
}

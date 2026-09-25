## This is a test WDL file for if-then-else expressions
version 1.0

workflow if_then_else_exprs {
    input {
        Int a
        Int b
        Boolean foo
        Boolean bar
    }

    Int c = (if (a < b) then a else b)

    Int d = if (a < b) then a else b

    Int qaz = if foo
        then if bar
            then c
            else if c == d
            then c - d
            else b
        else c + d

    call empty { input:
        arg = if foo
            then if bar
                then c
                else if c == d
                then c - d
                else b
            else c + d,
    }

    output {
        Int result = c
        Int other_result = qaz
    }
}

task empty {
    input {
        Int arg
    }

    command <<<
    >>>
}

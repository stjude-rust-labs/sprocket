version 1.4

task add {
    input {
        Int lhs
        Int rhs
    }

    command <<<>>>

    output {
        Int out = lhs + rhs
    }
}

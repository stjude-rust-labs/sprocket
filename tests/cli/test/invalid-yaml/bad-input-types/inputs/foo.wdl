version 1.3

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
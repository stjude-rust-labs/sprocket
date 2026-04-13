version 1.3

task add_1 {
    input {
        Int number = 30
    }

    command <<<>>>

    output {
        Int result = number + 1
    }
}

workflow add {
    input {
        Int? number
    }

    # `add_1.number` has a specified default, so we should be fine to pass in
    # an unspecified `Int?`.
    call add_1 { number }

    # Same thing, but with an expression
    call add_1 as add_2 { number = number }

    output {
        Int result = add_1.result
        Int result2 = add_2.result
    }
}

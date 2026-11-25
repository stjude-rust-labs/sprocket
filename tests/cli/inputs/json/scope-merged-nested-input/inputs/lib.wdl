version 1.4

task greet {
    input {
        String name
        String punctuation = "!"
    }

    command <<<>>>

    output {
        String message = "hello, ~{name}~{punctuation}"
    }
}

workflow sub {
    input {
        Int x = 1
    }

    call greet { name = "sub-~{x}" }

    output {
        String message = greet.message
    }

    hints {
        allow_nested_inputs: true
    }
}

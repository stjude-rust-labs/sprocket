version 1.3

workflow only_hints {
    hints {
        allow_nested_inputs: true
    }
}

workflow only_output {
    output {
        Int answer = 42
    }
}

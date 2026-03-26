## This is a test of attempting to override an explicitly specified input.

version 1.3

task foo {
    input {
        String x = "hi"
    }

    command <<<>>>
}

workflow bar {
    hints {
        allow_nested_inputs: true
    }

    call foo {
        x = "hello"
    }
}

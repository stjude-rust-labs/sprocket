## This is a test of a call input being present.
## No error should be present.

version 1.2

task foo {
    input {
        String a
        String b
    }

    command <<<>>>
}

workflow bar {
    hints {
        allow_nested_inputs: true
    }

    call foo {
        a = "Hello"
    }
}

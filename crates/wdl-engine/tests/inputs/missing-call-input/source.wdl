## This test checks for a missing required input to a call.

version 1.1

task foo {
    input {
        String x
    }

    command <<<>>>
}

workflow test {
    meta {
        allowNestedInputs: true
    }

    call foo
}

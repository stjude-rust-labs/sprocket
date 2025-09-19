## This is a test of an unknown nested input.

version 1.1

task foo {
    command <<<>>>
}

workflow bar {
    meta {
        allowNestedInputs: true
    }

    call foo
}

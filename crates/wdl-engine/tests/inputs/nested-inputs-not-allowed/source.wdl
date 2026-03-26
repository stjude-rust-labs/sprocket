## This is a test of an unknown call in a workflow input.

version 1.1

task foo {
    input {
        Int bar = 0
    }

    command <<<>>>
}

workflow test {
    call foo
}

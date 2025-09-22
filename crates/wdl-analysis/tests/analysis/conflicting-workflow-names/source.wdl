## This is a test of having conflicting workflow names.

version 1.1

task foo {
    command <<<>>>
}

workflow foo {}
workflow bar {}
workflow bar {}

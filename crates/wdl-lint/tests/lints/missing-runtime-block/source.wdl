# This is a test of the `missing_runtime_block` lint

version 1.1

task bad {
    command <<<>>>
}

task good {
    runtime {

    }

    command <<<>>>
}

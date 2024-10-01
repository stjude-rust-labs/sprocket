#@ except: DescriptionMissing

## This is a test of the `missing_runtime_block` lint

version 1.1

task bad {
    meta {}

    command <<<>>>

    output {}
}

task good {
    meta {}

    command <<<>>>

    output {}

    runtime {
    }
}

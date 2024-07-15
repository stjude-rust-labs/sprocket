#@ except: DescriptionMissing, RuntimeSectionKeys, SectionOrdering
## This is a test of the `missing_runtime_block` lint

version 1.1

task bad {
    meta {}
    output {}
    command <<<>>>
}

task good {
    meta {}
    output {}
    runtime {

    }

    command <<<>>>
}

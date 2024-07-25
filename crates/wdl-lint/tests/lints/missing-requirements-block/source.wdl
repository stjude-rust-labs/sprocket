#@ except: BlankLinesBetweenElements, DescriptionMissing, SectionOrdering

version 1.2

task bad {
    meta {}
    output {}
    command <<<>>>
}

task good {
    meta {}
    output {}
    command <<<>>>

    requirements {

    }
}

task deprecated_runtime {
    meta {}
    output {}
    command <<<>>>

    # This `runtime` section should be flagged as deprecated.
    runtime {

    }
}

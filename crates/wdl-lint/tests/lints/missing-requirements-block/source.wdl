#@ except: DescriptionMissing

version 1.2

task bad {
    meta {}

    command <<<>>>

    output {}
}

task good {
    meta {}

    command <<<>>>

    output {}

    requirements {
    }
}

task deprecated_runtime {
    meta {}

    command <<<>>>

    output {}

    # This `runtime` section should be flagged as deprecated.
    runtime {
    }
}

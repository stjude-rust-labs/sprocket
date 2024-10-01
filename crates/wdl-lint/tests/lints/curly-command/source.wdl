#@ except: DescriptionMissing, RuntimeSectionKeys

## This is a test of the `NoCurlyCommands` lint

version 1.1

task bad {
    meta {}

    parameter_meta {}

    command {
        echo "Hello, World!"
    }

    output {}

    runtime {}
}

task good {
    meta {}

    parameter_meta {}

    command <<<
        echo "Hello, World!"
    >>>

    output {}

    runtime {}
}

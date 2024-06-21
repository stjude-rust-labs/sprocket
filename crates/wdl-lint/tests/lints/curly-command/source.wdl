## This is a test of the `NoCurlyCommands` lint

version 1.1

task bad {
    meta {}
    parameter_meta {}

    runtime {}
    command {
        echo "Hello, World!"
    }

    output {}
}

task good {
    meta {}
    parameter_meta {}

    runtime {}
    command <<<
        echo "Hello, World!"
    >>>

    output {}
}

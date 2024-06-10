## This is a test of the `NoCurlyCommands` lint

version 1.1

task bad {
    runtime {}
    command {
        echo "Hello, World!"
    }
}

task good {
    runtime {}
    command <<<
        echo "Hello, World!"
    >>>
}

## Test case to ensure that requirements can be overridden on the CLI
version 1.3

task repro {
    requirements {
        memory: "4 MiB"
    }

    command <<<
        echo ~{task.memory}
    >>>

    output {
        String out = read_string(stdout())
    }
}

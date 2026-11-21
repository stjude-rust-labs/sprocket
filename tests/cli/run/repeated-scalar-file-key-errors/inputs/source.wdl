version 1.4

task greet {
    input {
        File file
    }

    command <<<
        cat ~{file}
    >>>

    output {
        String message = read_string(stdout())
    }

    requirements {
        container: "ubuntu:latest"
    }
}

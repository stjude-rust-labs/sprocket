version 1.3

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

version 1.3

task greet {
    input {
        String name
    }

    command <<<
        echo "hello ~{name}"
    >>>

    output {
        String message = read_string(stdout())
    }

    requirements {
        container: "ubuntu:latest"
    }
}

version 1.4

task hello {
    input {
        String name
    }

    command <<<
        echo "hello, ~{name}"
    >>>

    output {
        String greeting = read_string(stdout())
    }

    requirements {
        container: "ubuntu:latest"
    }
}

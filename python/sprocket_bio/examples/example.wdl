version 1.4

task say_hello {
    input {
        String greeting
    }

    command <<<
        echo "~{greeting}, world!"
    >>>

    output {
        String out = read_string(stdout())
    }

    requirements {
        container: "ubuntu:latest"
    }
}

version 1.4

task my_task {
    input {
        String name
        Int count
    }

    command <<<
        echo "~{name} ~{count}"
    >>>

    output {
        String result = read_string(stdout())
    }

    requirements {
        container: "ubuntu:latest"
    }
}

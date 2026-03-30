version 1.3

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

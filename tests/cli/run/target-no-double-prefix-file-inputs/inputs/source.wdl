version 1.4

task my_task {
    input {
        String name
        Int count
    }

    command <<<
        echo "~{name} ~{count}"
    >>>

    requirements {
        container: "ubuntu:latest"
    }
}

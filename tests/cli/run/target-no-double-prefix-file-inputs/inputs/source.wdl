version 1.3

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

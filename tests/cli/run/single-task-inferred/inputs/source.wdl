version 1.4

task only_task {
    command <<<
        echo "hello"
    >>>

    requirements {
        container: "ubuntu:latest"
    }
}

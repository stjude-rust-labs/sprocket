version 1.3

task only_task {
    command <<<
        echo "hello"
    >>>

    requirements {
        container: "ubuntu:latest"
    }
}

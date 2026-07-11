version 1.3

task hello {
    command <<<
        echo "hello"
    >>>

    requirements {
        container: "ubuntu:latest"
    }
}

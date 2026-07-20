version 1.2

task hello {
    command <<<
        echo "hello"
    >>>

    requirements {
        container: "ubuntu:24.04"
    }
}

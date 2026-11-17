version 1.3

task to_be_locked {
    command <<<
        cat /etc/lsb-release
    >>>

    requirements {
        container: "ubuntu:latest"
    }
}
version 1.4

task to_be_locked {
    command <<<
        cat /etc/lsb-release
    >>>

    requirements {
        container: "ubuntu:latest"
    }
}
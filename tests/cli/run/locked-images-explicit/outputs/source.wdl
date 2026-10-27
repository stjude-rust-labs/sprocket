version 1.3

task to_be_locked {
    command <<<
        cat /etc/lsb-release
    >>>

    requirements {
        # Should be overwritten with ubuntu mantic (much older than latest)
        container: "ubuntu:latest"
    }
}
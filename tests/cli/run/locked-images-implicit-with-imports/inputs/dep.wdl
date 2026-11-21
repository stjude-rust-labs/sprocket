version 1.4

task print_release {
    command <<<
        cat /etc/lsb-release
    >>>

    requirements {
        # Should be overwritten with ubuntu mantic (much older than latest)
        container: "ubuntu:latest"
    }
}
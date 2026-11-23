## This test ensures the working directory is writable even if the container uses a different user
version 1.4

task test {
    command <<<
        touch foo
    >>>

    requirements {
        container: "ghcr.io/multiqc/multiqc:v1.31"
    }
}

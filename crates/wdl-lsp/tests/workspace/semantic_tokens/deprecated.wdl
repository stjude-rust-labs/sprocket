version 1.2

task foo {
    runtime {
        docker: "ubuntu:latest"
    }

    requirements {
        docker: "ubuntu:latest"
    }
}
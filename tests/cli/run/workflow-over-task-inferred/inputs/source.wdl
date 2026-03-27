version 1.3

workflow my_workflow {
    call my_task
}

task my_task {
    command <<<
        echo "hello"
    >>>

    requirements {
        container: "ubuntu:latest"
    }
}

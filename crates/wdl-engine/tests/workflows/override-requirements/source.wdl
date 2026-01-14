version 1.3

task t {
    command <<<
        echo "~{if task.container == None then "ubuntu:focal" else task.container}"
        echo "~{task.cpu}"
        echo "~{task.memory}"
    >>>

    # The requirements here should be overwritten by the inputs
    requirements {
        container: "ubuntu:latest"
        cpu: 0.1
        memory: 100
    }

    output {
        String out = read_string(stdout())
    }
}

workflow w {
    call t

    output {
        String out = t.out
    }
}

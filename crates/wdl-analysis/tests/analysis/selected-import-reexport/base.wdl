version 1.4

task do_task {
    command <<<
        echo 1
    >>>

    output {
        Int out = read_int(stdout())
    }
}

workflow do_flow {
    output {
        Int out = 2
    }
}

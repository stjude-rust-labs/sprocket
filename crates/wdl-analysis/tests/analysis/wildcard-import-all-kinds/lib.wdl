version 1.4

struct Record {
    Int value
}

enum State {
    Ready,
    Done
}

task run_task {
    command <<<
        echo 1
    >>>

    output {
        Int out = read_int(stdout())
    }
}

workflow run_workflow {
    output {
        Int out = 2
    }
}

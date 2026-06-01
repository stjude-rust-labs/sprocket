version 1.3

task super_slow_task {
    command <<<
        sleep 20
    >>>
}

task super_fast_task {
    command <<<
        exit 1
    >>>
}

workflow failing_workflow {
    call super_fast_task
}

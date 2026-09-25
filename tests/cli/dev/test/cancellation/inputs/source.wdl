# Ensures that one task's failure (and thus cancellation) doesn't cancel any
# of the other running tasks.
#
# See https://github.com/stjude-rust-labs/sprocket/pull/891

version 1.3

task super_slow_task {
    command <<<
        sleep 5
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

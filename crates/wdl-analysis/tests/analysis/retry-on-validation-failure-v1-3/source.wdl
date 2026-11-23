version 1.3

task legacy {
    command <<<
        echo "~{task.retry_on_validation_failure}"
    >>>

    output {
        Boolean r = task.retry_on_validation_failure
        Boolean? prev_r = task.previous.retry_on_validation_failure
    }

    requirements {
        max_retries: if select_first([task.previous.retry_on_validation_failure, true]) then 1 else 0
    }

    hints {
        short_task: select_first([task.previous.retry_on_validation_failure, false])
    }
}

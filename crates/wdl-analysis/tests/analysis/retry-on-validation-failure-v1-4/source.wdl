version 1.4

task valid {
    command <<<
        echo "~{task.retry_on_validation_failure} ~{task.previous.retry_on_validation_failure}"
    >>>

    output {
        Boolean r = task.retry_on_validation_failure
        Boolean? prev_r = task.previous.retry_on_validation_failure

        validation {
            r || !r: "ok (~{task.retry_on_validation_failure})",
        }
    }

    requirements {
        max_retries: if select_first([task.previous.retry_on_validation_failure, true]) then 1 else 0
        retry_on_validation_failure: !select_first([task.previous.retry_on_validation_failure, false])
    }

    hints {
        short_task: select_first([task.previous.retry_on_validation_failure, false])
    }
}

task valid_runtime {
    command <<<>>>

    runtime {
        retry_on_validation_failure: false
    }
}

task invalid_types_and_scopes {
    command <<<>>>

    requirements {
        retry_on_validation_failure: 1
        max_retries: if task.retry_on_validation_failure then 1 else 0
    }
}

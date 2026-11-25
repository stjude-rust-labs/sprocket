version 1.4

task valid_preemptible {
    command <<<
        echo "~{task.preemptible} ~{task.preemptible_only}"
    >>>

    output {
        Int p = task.preemptible
        Boolean po = task.preemptible_only
        Boolean? prev_po = task.previous.preemptible_only
    }

    requirements {
        preemptible_only: select_first([task.previous.preemptible_only, true])
    }

    hints {
        preemptible: 2
    }
}

task valid_runtime_preemptible_only {
    command <<<>>>

    runtime {
        preemptible_only: false
    }
}

task invalid_types_and_scopes {
    command <<<>>>

    output {
        Int? prev_p = task.previous.preemptible
    }

    requirements {
        preemptible_only: 1
        cpu: if task.preemptible_only then 1 else 2
        max_retries: task.preemptible
    }

    hints {
        preemptible: true
    }
}

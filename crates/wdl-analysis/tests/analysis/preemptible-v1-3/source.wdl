version 1.3

task legacy_preemptible_hint {
    command <<<
        echo "~{task.preemptible} ~{task.preemptible_only}"
    >>>

    output {
        Int p = task.preemptible
        Boolean po = task.preemptible_only
    }

    hints {
        # Custom hint with non-Int value remains allowed prior to WDL 1.4
        preemptible: true
    }
}

version 1.3

task likes_to_fail {
    command <<<
        if (( ~{ task.attempt } != 1 )); then
            exit 1
        fi
    >>>

    requirements {
        max_retries: 1
    }
}
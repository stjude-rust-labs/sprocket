## This is a test of retrying a failed task execution.

version 1.2

task test {
    requirements {
        # There will be at most three retries (4 attempts) of the task
        max_retries: 3
    }

    command <<<
        # Fail if the attempt is not the 4th attempt (attempt = 3)
        if (( ~{ task.attempt } != 3 )); then
            exit 1
        fi

        echo 'attempt ~{ task.attempt } was successful!' > done.txt
    >>>

    output {
        File done = "done.txt"
    }
}

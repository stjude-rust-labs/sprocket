# This is a test of too many runtime sections in a task.

version 1.1

task t {
    runtime {

    }

    runtime {

    }

    command <<<>>>
}

# This duplicate task should be ignored.
task t {
    runtime {

    }

    runtime {

    }

    command <<<>>>
}

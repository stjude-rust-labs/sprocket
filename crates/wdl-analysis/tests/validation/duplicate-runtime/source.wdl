# This is a test of too many runtime sections in a task.

version 1.1

task t {
    runtime {

    }

    runtime {

    }

    command <<<>>>
}

# A duplicate task should trigger a single error and then be ignored.
task t {
    runtime {

    }

    runtime {

    }

    command <<<>>>
}

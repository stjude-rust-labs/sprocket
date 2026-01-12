# This is a test of too many requirements sections in a task.

version 1.3

task t {
    requirements {

    }

    requirements {

    }

    command <<<>>>
}

# A duplicate task should trigger a single error and then be ignored.
task t {
    requirements {

    }

    requirements {

    }

    command <<<>>>
}

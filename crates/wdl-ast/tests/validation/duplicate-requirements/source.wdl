# This is a test of too many requirements sections in a task.

version 1.2

task t {
    requirements {

    }

    requirements {

    }

    command <<<>>>
}

# This duplicate task should be ignored.
task t {
    requirements {

    }

    requirements {

    }

    command <<<>>>
}

# This is a test of too many hints sections in a task or workflow.

version 1.2

task t {
    hints {

    }

    hints {

    }

    command <<<>>>
}

# A duplicate task should trigger a single error and then be ignored.
task t {
    hints {

    }

    hints {

    }

    command <<<>>>
}

workflow w {
    hints {

    }

    hints {

    }
}

# This is a test of too many hints sections in a task or workflow.

version 1.2

task t {
    hints {

    }

    hints {

    }

    command <<<>>>
}

# This duplicate task should be ignored.
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

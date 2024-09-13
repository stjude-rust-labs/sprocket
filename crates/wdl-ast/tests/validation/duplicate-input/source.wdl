# This is a test of too many input sections in a task and workflow.

version 1.1

task t {
    input {

    }

    input {

    }

    command <<<>>>
}

# This duplicate task should be ignored.
task t {
    input {

    }

    input {

    }

    command <<<>>>
}

workflow w {
    input {

    }

    input {

    }
}

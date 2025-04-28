# This is a test of too many input sections in a task and workflow.

version 1.1

task t {
    input {

    }

    input {

    }

    command <<<>>>
}

# A duplicate task should trigger a single error and then be ignored.
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

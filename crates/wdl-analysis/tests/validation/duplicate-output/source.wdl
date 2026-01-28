# This is a test of too many output sections in a task and workflow.

version 1.1

task t {
    output {
        String a = ""
    }

    output {
        String b = ""
    }

    command <<<>>>
}

# A duplicate task should trigger a single error and then be ignored.
task t {
    output {
        String a = ""
    }

    output {
        String b = ""
    }

    command <<<>>>
}

workflow w {
    output {
        String a = ""
    }

    output {
        String b = ""
    }
}

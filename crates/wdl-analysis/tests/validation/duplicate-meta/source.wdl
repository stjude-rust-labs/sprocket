# This is a test of too many meta sections in a task, workflow, and struct definition.

version 1.2

task t {
    meta {

    }

    meta {

    }

    command <<<>>>
}

# A duplicate task should trigger a single error and then be ignored.
task t {
    meta {

    }

    meta {

    }

    command <<<>>>
}

workflow w {
    meta {

    }

    meta {

    }
}

struct X {
    String x
    
    meta {

    }

    meta {

    }
}

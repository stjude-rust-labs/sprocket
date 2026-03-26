# This is a test of too many param meta sections in a task, workflow, and struct definition.

version 1.3

task t {
    parameter_meta {

    }

    parameter_meta {

    }

    command <<<>>>
}

# A duplicate task should trigger a single error and then be ignored.
task t {
    parameter_meta {

    }

    parameter_meta {

    }

    command <<<>>>
}

workflow w {
    parameter_meta {

    }

    parameter_meta {

    }
}

struct X {
    String x
    
    parameter_meta {

    }

    parameter_meta {

    }
}

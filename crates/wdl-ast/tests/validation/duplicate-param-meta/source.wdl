# This is a test of too many param meta sections in a task, workflow, and struct definition.

version 1.2

task t {
    parameter_meta {

    }

    parameter_meta {

    }

    command <<<>>>
}

# This duplicate task should be ignored.
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

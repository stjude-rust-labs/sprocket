# This is a test of too many meta sections in a task, workflow, and struct definition.

version 1.2

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

## This is a test for a missing description in a `meta` section.

version 1.2

task foo {
    meta {

    }

    command <<<>>>

    output {

    }

    runtime {

    }
}

workflow bar {
    meta {

    }

    output {

    }
}

struct Baz {
    String x

    meta {

    }

    parameter_meta {
        x: "foo"
    }
}

## This is a test for a missing description in a `meta` section.

version 1.2

#@ except: RequirementsSection
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
    meta {
    }

    parameter_meta {
        x: "foo"
    }

    String x
}

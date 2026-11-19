# This is a test of reserved keywords used as arbitrary keys and member names.

version 1.3

struct Foo {
    String x

    meta {
        enum: "a"
        import: "b"
    }

    parameter_meta {
        x: {
            enum: "c",
            struct: "d",
        }
    }
}

task test {
    meta {
        import: "abc"
        None: "xyz"
        enum: "e"
        version: "v"
        from: "f"
        nested: {
            if: true,
            Int: 1,
        }
    }

    parameter_meta {
        enum: "g"
    }

    command <<<
        echo "~{task.meta.import}" "~{task.meta.enum}" "~{task.meta.nested.if}"
    >>>

    output {
        Object o = object {
            import: 1,
            enum: 2,
            None: 3,
        }
        Int a = o.import
        Int b = o.enum
        Int c = o.None
    }

    requirements {
        container: "ubuntu:latest"
    }

    hints {
        enum: 1
        import: 2
    }
}

workflow wf {
    meta {
        enum: "h"
        workflow: "i"
    }

    hints {
        enum: "j"
        call: {
            import: 1,
            enum: 2,
        }
    }
}

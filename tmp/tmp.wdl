version 1.0

task inner {
    input {
        File foo
        File bar
    }

    command <<<
        cat ~{foo} ~{bar} > out.txt
    >>>

    output {
        File foo_and_bar = "out.txt"
    }
}

workflow outer {
    input {
        File foo
    # no bar!
    }

    call inner { input:
        foo,
    }  # bar must be specified as a nested input
    call inner as duplicate { input:
        foo = inner.foo_and_bar,
        bar = inner.foo_and_bar,
    }

    output {
        File duplicated = duplicate.foo_and_bar
    }
}

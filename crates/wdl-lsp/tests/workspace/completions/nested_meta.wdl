version 1.3

task MetaTop {
    meta {
        root: {
            child_a: "a",
            child_b: "b"
        }
    }

    command <<<
        true
    >>>

    output {
        String top = task.meta.
    }
}

task MetaNested {
    meta {
        root: {
            child_a: "a",
            child_b: "b"
        }
    }

    command <<<
        true
    >>>

    output {
        String nested = task.meta.root.
    }
}

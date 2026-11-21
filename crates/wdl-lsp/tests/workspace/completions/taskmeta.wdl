version 1.4

task nested_meta {
    meta {
        nested_obj: {
            alpha: "a",
            beta: 1,
            inner_obj: {
                gamma: true,
            },
        },
        sibling: "s",
    }

    command <<<
        echo "hello"
    >>>

    output {
        String child = task.meta.nested_obj.
    }
}

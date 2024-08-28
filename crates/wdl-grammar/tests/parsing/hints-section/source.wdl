## This is a test of parsing task hints sections.

version 1.2

task foo {
    hints {
        a: hints {
            a: "a",
            b: 1,
            c: 1.0,
            d: [1, 2, 3],
        }
        inputs: input {
            foo: hints {
                a: "a",
                b: "b",
                c: "c",
            },
            baz.bar.qux: hints {
                foo: "foo",
                bar: "bar",
                baz: "baz",
            },
        }
        c: "foo"
        d: 1
        outputs: output {
            foo: hints {
                a: "a",
                b: "b",
                c: "c",
            },
            baz.bar.qux: hints {
                foo: "foo",
                bar: "bar",
                baz: "baz",
            },
        }
    }
}

workflow bar {
    hints {
        a: hints {
            a: "a",
            b: 1,
            c: 1.0,
            d: [1, 2, 3],
        }
        inputs: input {
            foo: hints {
                a: "a",
                b: "b",
                c: "c",
            },
            baz.bar.qux: hints {
                foo: "foo",
                bar: "bar",
                baz: "baz",
            }
        }
        c: "foo"
        d: 1
        outputs: output {
            foo: hints {
                a: "a",
                b: "b",
                c: "c",
            },
            baz.bar.qux: hints {
                foo: "foo",
                bar: "bar",
                baz: "baz",
            }
        }
    }
}

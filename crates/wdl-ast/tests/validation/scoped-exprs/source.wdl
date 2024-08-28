## This is a test of ensuring certain expressions can only be used in particular scopes.

version 1.2

task ok {
    command <<<>>>

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

task bad {
    command <<<>>>

    Int a = hints {
        foo: "bar"
    }

    Int b = input {
        foo: "bar"
    }

    Int c = output {
        foo: "bar"
    }

    hints {
        ok: hints {
            bad: hints {
                bad: input {
                    bad: output {

                    }
                }
            }
        }
        inputs: input {
            ok: hints {
                bad: hints {

                }
            },
            inputs: input {
                a: input {

                },
                b: hints {
                    a: input {

                    },
                    b: output {

                    },
                    c: hints {

                    },
                },
                c: output {

                },
            }
        }
        outputs: output {
            a: input {

            },
            b: hints {
                a: input {

                },
                b: output {

                },
                c: hints {

                },
            },
            c: output {

            }
        }
    }
}

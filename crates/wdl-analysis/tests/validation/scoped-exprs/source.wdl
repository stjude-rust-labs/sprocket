## This is a test of ensuring certain expressions can only be used in particular scopes.

version 1.3

task ok {
    input {
        String foo
        String bar
        BazBarQux baz
    }

    output {
        String foo2 = "foo"
        BazBarQux baz2 = baz
    }

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
            foo2: hints {
                a: "a",
                b: "b",
                c: "c",
            },
            baz2.bar.qux: hints {
                foo: "foo",
                bar: "bar",
                baz: "baz",
            },
        }
    }
}

task bad {
    input {
        String a
        String b
        String c
        String ok
        String bad
        String inputs
    }

    output {
        String g = "foo"
        String h = "bar"
        String i = "baz"
    }

    command <<<>>>

    Int d = hints {
        foo: "bar"
    }

    Int e = input {
        foo: "bar"
    }

    Int f = output {
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
            g: input {

            },
            h: hints {
                a: input {

                },
                b: output {

                },
                c: hints {

                },
            },
            i: output {

            }
        }
    }
}

struct BazBarQux {
    BarQux bar
}

struct BarQux {
    String qux
}

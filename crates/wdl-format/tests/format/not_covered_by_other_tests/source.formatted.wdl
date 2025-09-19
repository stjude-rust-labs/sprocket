## This is a WDL file with Nodes not covered by other tests
version 1.2

task test1 {
    meta {
    }

    parameter_meta {
    }

    input {
        String? name = None
        Float exponent = 2.7 ** 3
    }

    Pair[String, Float] literal = ("hello", 3.14 - 6.8)

    Boolean flag = true
    Int modulo = 42 % 7

    command  # my command block
    <<<
        echo 'hello ~{default="world" name}'
        echo '~{true="good" false="bad" flag}bye'
    >>>

    output {
        Int math = 42 / 7
    }

    hints {
        inputs: input {
            a: hints {
                foo: "bar",
            }
        }
        f: [
            1,
            2,
            3,
        ]
        g: {
            foo: "bar",
        }
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

workflow test2 {
    Pair[String, Float] literal = ("hello", 3.14 - 6.8)

    output {
        Int math = 42 / 7
    }

    hints {
        allow_nested_inputs: true
        a: true
        b: 1
        c: 1.0
        d: -1
        e: "foo"
        f: [
            1,
            2,
            3,
        ]
        g: {
            foo: "bar"
        }
    }
}

version 1.3

task foo {
    command <<<
    >>>

    hints {
        inputs: input {
            foo.bar: hints {
            },
            baz: hints {
                localization_optional: true,
            },
            a.b.c: hints {
                localization_optional: false,
            },
        }
        outputs: output {
            out.field: hints {
                x: 1,
            },
        }
    }
}

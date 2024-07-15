## This is a test of using a hint section in a 1.2 document
## There should be no diagnostics emitted.

version 1.2

task foo {
    command <<<>>>

    hints {
        inputs: input {
            a: hints {
                foo: "bar"
            }
        }
    }
}

workflow bar {
    hints {
        inputs: input {
            a: hints {
                foo: "bar"
            }
        }
    }
}

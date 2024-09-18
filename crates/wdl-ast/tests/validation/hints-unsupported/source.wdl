## This is a test of using a hint section in a 1.1 document

version 1.1

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
        allow_nested_inputs: true
    }
}

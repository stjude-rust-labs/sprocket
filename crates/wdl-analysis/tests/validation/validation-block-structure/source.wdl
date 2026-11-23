## This is a test of the structure of validation blocks.

version 1.4

task t {
    input {
        Int a

        validation {
            a > 0: "a must be positive",
        }

        validation {
            a < 10: "duplicate validation block",
        }
    }

    command <<<>>>

    output {
        validation {
            out > 0: "out must be positive",
        }

        Int out = a
    }
}

workflow w {
    input {
        validation {
            a > 0: "a must be positive",
        }

        Int a
        Int b
    }

    output {
        Int out = a + b

        validation {
            out > 0: "out must be positive",
        }

        validation {
            out < 10: "duplicate validation block",
        }
    }
}

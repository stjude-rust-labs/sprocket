## This is a test of malformed validation blocks.

version 1.4

task missing_comma {
    input {
        Int a
        Int b

        validation {
            a > 0: "a must be positive"
            b > 0: "b must be positive",
        }
    }

    command <<<>>>
}

task missing_colon {
    input {
        Int a

        validation {
            a > 0 "a must be positive",
            a < 10: "a must be less than 10",
        }
    }

    command <<<>>>

    output {
        Int out = a

        validation {
            out > 0: "out must be positive",
            out: ,
        }
    }
}

workflow missing_message {
    input {
        Int a

        validation {
            a > 0:,
        }

        Int b
    }
}

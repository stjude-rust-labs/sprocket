## This is a test of validation blocks in a pre-1.4 document.

version 1.3

task t {
    input {
        Int a

        validation {
            a > 0: "a must be positive",
        }
    }

    command <<<>>>

    output {
        Int out = a

        validation {
            out > 0: "out must be positive",
        }
    }
}

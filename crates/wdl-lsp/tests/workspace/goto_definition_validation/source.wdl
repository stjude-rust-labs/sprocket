version 1.4

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

workflow w {
    input {
        String name

        validation {
            name != "": "name must not be empty",
        }
    }

    call t { a = 1 }

    output {
        Int result = t.out

        validation {
            result > 0: "result must be positive ~{name}",
        }
    }
}

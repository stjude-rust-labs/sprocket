## This is a test of type mismatches in validation clauses.

version 1.4

task t {
    input {
        Int a
        Array[String] b = []

        validation {
            a: "condition is not a Boolean",
            a > 0: b,
            "nope": 1,
            a > 0: a,
        }
    }

    command <<<>>>

    output {
        Int out = a

        validation {
            out: "condition is not a Boolean",
            out > 0: [out],
        }
    }
}

workflow w {
    input {
        Map[String, Int] m = {}

        validation {
            m: "condition is not a Boolean",
            length(m) > 0: m,
        }
    }

    output {
        Int out = length(m)

        validation {
            "x": "condition is not a Boolean",
            out > 0: { "a": out },
        }
    }
}

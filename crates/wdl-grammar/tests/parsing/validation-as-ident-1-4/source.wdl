## This is a test of `validation` being used as an identifier in WDL 1.4.

version 1.4

struct validation {
    Int value
}

task example {
    input {
        String validation = "ok"
        validation v = v
    }

    command <<<>>>

    output {
        validation out = v
    }
}

workflow test {
    input {
        Array[Int] items = []
    }

    scatter (validation in items) {
        Int value = validation
    }
}

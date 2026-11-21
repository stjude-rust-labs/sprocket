## This is a test of `validation` being used as an identifier prior to WDL 1.4.

version 1.3

struct validation {
    Int value
}

struct Wrapper {
    validation inner
    Array[validation] many
    Map[String, validation] by_name
    Pair[validation, validation?] pair
}

task example {
    input {
        String validation = "ok"
        validation v = validation { value: 1 }
        validation? maybe
        env String other = validation
    }

    command <<<
        echo ~{validation}
    >>>

    validation private = validation { value: 2 }

    output {
        validation out = private
        String validation_text = validation
    }
}

workflow test {
    input {
        Array[validation] items = []
    }

    scatter (validation in items) {
        Int value = validation.value
        validation copy = validation
    }

    if (true) {
        validation nested = validation { value: 3 }
    }

    validation decl = validation { value: 4 }

    call example as validation_call { validation = "hi" }

    output {
        Array[validation] copies = copy
        validation result = decl
    }
}

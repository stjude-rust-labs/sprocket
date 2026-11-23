version 1.3

struct validation {
    String name
}

workflow test {
    input {
        validation validation
        Array[validation] validations = []
    }

    validation v = validation {
        name: "x",
    }

    scatter (validation in validations) {
        String n = validation.name
    }

    output {
        validation out = validation
    }
}

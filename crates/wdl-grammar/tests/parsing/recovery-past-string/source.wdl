# This is a test of recovery past an interpolation (i.e. a string).

version 1.1

task test {
    meta {
        "invalid": "~{value}"
        correct: "value"
    }
}

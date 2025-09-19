## This is a test of properly showing a string in a diagnostic

version 1.1

workflow test {
    "this ${'~{"string"}'} is unexpected!"
}

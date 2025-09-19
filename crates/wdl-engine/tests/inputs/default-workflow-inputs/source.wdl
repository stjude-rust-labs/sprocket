## This is a test of passing no inputs to a workflow with defaulted inputs.
## No error should be present in error.txt.

version 1.1

workflow test {
    input {
        String x = "hi"
        Int? y
    }
}

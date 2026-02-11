version 1.0

workflow compound_array_checks {
    input {
        Array[File] samples
    }

    output {
        Array[File] echoed = samples
    }
}

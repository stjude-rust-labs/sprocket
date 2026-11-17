version 1.3

workflow test_list {
    input {
        Directory directory
    }

    output {
        Pair[Array[File], Array[Directory]] entries = list(directory)
    }
}

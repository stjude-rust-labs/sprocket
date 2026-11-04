version 1.3

workflow test_exists {
    input {
        String path
    }

    output {
        Boolean path_exists = exists(path)
    }
}

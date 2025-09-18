## This is a test of an invalid default placeholder option at runtime
version 1.2

task test {
    command {
        echo '[1]' > array.json
    }

    output {
        # This should fail at runtime as the file does not contain a primitive
        String out = "~{default='nope' read_json("array.json")}"
    }
}

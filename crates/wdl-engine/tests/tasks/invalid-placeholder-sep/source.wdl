## This is a test of an invalid sep placeholder option at runtime
version 1.3

task test {
    command {
        echo '1' > not-array.json
    }

    output {
        # This should fail at runtime as the file does not contain an array
        String out = "~{sep=', ' read_json("not-array.json")}"
    }
}

## This is a test of an invalid true/false placeholder option at runtime
version 1.2

task test {
    command {
        echo '1' > not-bool.json
    }

    output {
        # This should fail at runtime as the file does not contain a boolean
        String out = "~{true='y' false='n' read_json("not-bool.json")}"
    }
}

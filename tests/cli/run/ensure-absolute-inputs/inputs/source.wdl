version 1.3

task test {
    input {
        File first
        File second
    }

    Float first_size = size(first)
    Float second_size = size(second)

    command <<<
        echo "the size of the first file is ~{ceil(first_size)} bytes"
        echo "the size of the second file is ~{ceil(second_size)} bytes"
    >>>

    output {
        String message = read_string(stdout())
    }
}

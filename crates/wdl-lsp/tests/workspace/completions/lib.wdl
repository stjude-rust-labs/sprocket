version 1.3

task greet {
    input {
        String name
    }

    command <<<
        echo "hello ~{name}"
    >>>

    output {
        String out = read_string(stdout())
    }
}

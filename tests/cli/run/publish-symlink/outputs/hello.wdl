version 1.2

task hello {
    command <<<
        echo "hello"
    >>>

    output {
        String greeting = read_string(stdout())
    }
}

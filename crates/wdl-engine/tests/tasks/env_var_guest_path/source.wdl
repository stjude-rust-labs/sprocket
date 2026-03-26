version 1.3

task test {
    input {
        env File f
    }

    command <<<
        cat $f
    >>>

    output {
        String out = read_string(stdout())
    }
}

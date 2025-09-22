version 1.2

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

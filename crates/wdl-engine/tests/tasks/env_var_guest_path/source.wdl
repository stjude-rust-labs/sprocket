version 1.4

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

version 1.4

task run {
    command <<<
        echo 1
    >>>

    output {
        Int out = read_int(stdout())
    }
}

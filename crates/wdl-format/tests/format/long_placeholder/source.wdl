version 1.2

task t {
    command <<<
        some_program --xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx ~{aaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbb}
        echo done
    >>>

    output {
        String s = "x"
    }
}

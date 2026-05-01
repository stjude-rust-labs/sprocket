version 1.3

task example {
    input {
        String from = "origin"
    }

    command <<<
        echo ~{from}
    >>>

    output {
        String out = from
    }
}

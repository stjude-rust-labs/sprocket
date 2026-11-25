version 1.4

task first {
    input {
        String value
    }

    command <<<
    >>>
}

workflow second {
    input {
        Int count
    }

    output {
        Int total = count
    }
}

workflow third {
    input {
        Int count
    }

    output {
        Int total = count
    }
}

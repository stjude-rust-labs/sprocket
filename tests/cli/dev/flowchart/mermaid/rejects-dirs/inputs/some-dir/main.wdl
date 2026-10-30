version 1.3

task produce {
    command <<< echo value >>>
    output { String value = read_string(stdout()) }
}

workflow main {
    call produce

    output {
        String value = produce.value
    }
}

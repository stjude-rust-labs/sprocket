version 1.3

task echo {
    input { String value }
    command <<< echo ~{value} >>>
    output { String out = read_string(stdout()) }
}

workflow child {
    input { String value }
    call echo { input: value = value }
    output { String out = echo.out }
}

version 1.2

task always_fails {
    command <<<
        exit 1
    >>>

    output {}
}

task always_succeeds {
    command <<<
        exit 0
    >>>
}

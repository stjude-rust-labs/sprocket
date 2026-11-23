version 1.4

task always_fails {
    command <<<
        exit 1
    >>>
}

task always_succeeds {
    command <<<
        exit 0
    >>>
}

version 1.2

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


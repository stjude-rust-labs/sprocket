## This is a test of having conflicting task names.

version 1.1

workflow foo {
    # Not OK
}

task foo {
    command <<<>>>
}

task bar {
    command <<<>>>
}

task bar {
    command <<<>>>
}

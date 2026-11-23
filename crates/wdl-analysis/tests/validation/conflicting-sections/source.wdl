## This is a test of conflicting sections.

version 1.4

task foo {
    command <<<>>>

    runtime {}

    requirements {}
}

task bar {
    command <<<>>>

    requirements {}

    runtime {}
}

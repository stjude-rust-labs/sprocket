## This checks to ensure that validation and analysis of duplicate tasks
## is effectively ignored beyond reporting the duplicate

version 1.1

task foo {
    # OK
    Int i = 0

    command <<<>>>
}

# NOT OK (not ignored)
task foo {
    # NOT OK (but ignored)
    Int i = "0"

    command <<<>>>

    # NOT OK (but ignored)
    command <<<>>>
}

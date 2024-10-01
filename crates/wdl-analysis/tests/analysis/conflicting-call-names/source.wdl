## This is a test of conflicting call names.

version 1.1

import "baz.wdl"

task foo {
    command <<<>>>
}

task bar {
    command <<<>>>
}

task x {
    command <<<>>>
}

task ok {
    command <<<>>>
}

workflow test {
    Int my_int = 0      # FIRST
    call my_int         # NOT OK

    call foo            # FIRST
    call foo            # NOT OK

    call foo as bar     # FIRST
    call foo as bar     # NOT OK

    call bar            # NOT OK

    call baz.bar        # NOT OK
    call baz.baz        # FIRST

    call foo as baz     # NOT OK

    scatter (x in []) {
        call foo        # NOT OK
        call x          # NOT OK
        call ok         # OK
    }

    call x              # NOT OK
    call ok             # NOT OK
}

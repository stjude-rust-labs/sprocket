## This is a test of conflicting call names.

version 1.1

workflow test {
    Int my_int = 0      # FIRST
    call my_int         # OK

    call foo            # FIRST
    call foo            # NOT OK

    call foo as bar     # FIRST
    call foo as bar     # NOT OK

    call bar            # NOT OK

    call foo.bar        # NOT OK
    call foo.baz        # FIRST

    call foo as baz     # NOT OK

    scatter (x in []) {
        call foo        # NOT OK
        call x          # NOT OK
        call ok         # OK
    }

    call x              # NOT OK
    call ok             # NOT OK
}

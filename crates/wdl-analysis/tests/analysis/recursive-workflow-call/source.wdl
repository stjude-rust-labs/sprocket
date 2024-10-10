#@ except: UnusedCall
## This is a test of a recursive workflow call

version 1.1

workflow test {
    call test
}

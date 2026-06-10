## This is a test of a recursive workflow call

#@ except: UnusedCall

version 1.1

workflow test {
    call test
}

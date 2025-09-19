## This is a test of an unused call

version 1.1

task foo {
    command <<<>>>

    output {
        Int x = 0
    }
}

workflow test {
    # The call is never used
    call foo

    # This call is never used, but is excepted
    #@ except: UnusedCall
    call foo as bar
}

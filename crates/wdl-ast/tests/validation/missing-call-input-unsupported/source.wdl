## This is a test of a missing input keyword in a call body for WDL 1.1

version 1.1

workflow test {
    call foo { foo = bar }
}

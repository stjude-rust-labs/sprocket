## This is a test of a missing call input in WDL 1.2.
## There should be no diagnostics for this test.

version 1.2

workflow test {
    call foo { foo = bar }
}

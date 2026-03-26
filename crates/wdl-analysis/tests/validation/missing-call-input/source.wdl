## This is a test of a missing call input in WDL 1.2.
## There should be no diagnostics for this test.

version 1.3

workflow test {
    String bar = "bar"
    call foo { foo = bar }
}

task foo {
    input {
        String foo
    }

    command <<<>>>
}

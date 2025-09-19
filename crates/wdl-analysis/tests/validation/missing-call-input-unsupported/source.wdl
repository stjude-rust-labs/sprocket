## This is a test of a missing input keyword in a call body for WDL 1.1

version 1.1

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

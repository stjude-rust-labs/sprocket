## This is a test of optionally using `input`  in a call statement body.

version 1.3

workflow test {
    call foo { foo, bar = 1 }
}

## This is a test of using an optional input type from an imported document.
## No diagnostics should be generated.
## See https://github.com/stjude-rust-labs/wdl/pull/277#issuecomment-2562199340

version 1.1

import "foo.wdl"

workflow bar {
    input {
        Array[String]? bar
    }

    #@ except: UnusedCall
    call foo.foo { input: foo = bar }
}

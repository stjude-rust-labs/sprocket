version 1.4

import { run } from "lib.wdl"

workflow main {
    call run

    output {
        Int result = run.out
    }
}

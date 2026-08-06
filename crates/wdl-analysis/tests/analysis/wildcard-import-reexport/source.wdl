version 1.4

import * from "mid.wdl"

workflow main {
    call run

    output {
        Int result = run.out
    }
}


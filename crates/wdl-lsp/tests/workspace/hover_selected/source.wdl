version 1.4

import { add, run } from "lib.wdl"

workflow main {
    call add
    call run
}

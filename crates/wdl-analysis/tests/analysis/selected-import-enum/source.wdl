version 1.4

import { Status } from "lib.wdl"

workflow main {
    Status status = Status.Ready

    output {
        Status result = status
    }
}


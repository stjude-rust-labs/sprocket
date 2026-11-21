version 1.4

import * from "aliased_mid.wdl"

workflow main {
    input {
        Item rec
    }

    output {
        String id = rec.identifier
        Code state = Code.Ready
    }
}

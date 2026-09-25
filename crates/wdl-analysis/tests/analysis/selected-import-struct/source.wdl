version 1.4

import { Record } from "lib.wdl"

workflow main {
    Record record = Record {
        value: 1,
    }

    output {
        Int result = record.value
    }
}


version 1.4

import * from "lib.wdl"

task use_types {
    input {
        Record rec
        State state
    }

    command <<<>>>

    output {
        Int out = rec.value
        State result = state
    }
}

version 1.4

import * from "lib.wdl"

workflow main {
    call run_task
    call run_workflow

    Record record = Record {
        value: run_task.out,
    }
    State state = State.Ready

    output {
        Int combined = record.value + run_workflow.out
        State selected_state = state
    }
}

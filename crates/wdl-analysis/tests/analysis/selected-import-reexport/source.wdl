version 1.4

import { do_task } from "mid.wdl"
import { do_flow } from "mid.wdl"

workflow main {
    call do_task
    call do_flow

    output {
        Int task_result = do_task.out
        Int flow_result = do_flow.out
    }
}

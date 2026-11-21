version 1.4

import * from "lib.wdl"
import { greet as say_hi, sub as run_sub } from "lib.wdl"
import * from "mid.wdl"

workflow main {
    input {
        String who
    }

    call greet { name = who }
    call sub
    call say_hi as hi { name = "alias" }
    call run_sub as pipe { x = 2 }
    call relay_task
    call relay_flow as relayed

    output {
        String direct = greet.message
        String nested = sub.message
        String renamed = hi.message
        String renamed_nested = pipe.message
        String transitive = relay_task.message
        String transitive_nested = relayed.message
    }

    hints {
        allow_nested_inputs: true
    }
}

version 1.4

import { add, run } from "lib.wdl"
import * from "greetings.wdl"

workflow main {
    call add
    call run

    call say_hello {
        name = "John"
    }

    call do_greetings {
        name = "World"
    }

    output {
        String out = say_hello.stdout
        String wf_out = do_greetings.stdout
    }
}

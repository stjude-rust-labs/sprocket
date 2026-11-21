version 1.4

import { Record as Item, State as Code } from "shadow_base.wdl"

struct Record {
    Int identifier
}

enum State {
    Ready,
    Done,
}

workflow main {
    input {
        Item item
        Record local_rec
        Code code
    }

    output {
        Int id = item.identifier
        Int local_id = local_rec.identifier
        Boolean ready = code == Code.Ready
        State local_state = State.Ready
    }
}

version 1.4

import "lib.wdl" as lib
import "merged.wdl" # Should make `merged.tasks.nested_task` available under `merged.nested_task`

workflow namespace {
    call lib.
}

workflow merged_namespace {
    call merged.
}

# A form-1 (namespaced) import exposes the imported document's full surface,
# including tasks it re-exported into its own scope with a scope-merging
# import. `sort_task` is re-exported by `entry.wdl` and must be reachable as
# `entry.sort_task` from a consumer that imports `entry.wdl` by namespace.
version 1.4

import "entry.wdl"

workflow test {
    call entry.sort_task
    call entry.entry_local
}

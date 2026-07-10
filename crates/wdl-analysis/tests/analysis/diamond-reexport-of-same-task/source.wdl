# A diamond import: `align` reaches `source.wdl` through two paths — directly
# from `shared.wdl` and re-exported through `a.wdl`. Because both names denote
# the same underlying declaration in the same resolved source, this is not a
# conflict and needs no rename.
version 1.4

import * from "shared.wdl"
import * from "a.wdl"

workflow test {
    call align
    call a_local
}

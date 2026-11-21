version 1.4

import "second.wdl"

workflow third {
    call second.second
}
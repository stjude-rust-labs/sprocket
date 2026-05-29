version 1.3

import "second.wdl"

workflow third {
    call second.second
}
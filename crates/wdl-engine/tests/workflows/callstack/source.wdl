version 1.3

import "second.wdl"

workflow test {
    call second.test
}
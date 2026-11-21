version 1.4

import "second.wdl"

workflow test {
    call second.test
}
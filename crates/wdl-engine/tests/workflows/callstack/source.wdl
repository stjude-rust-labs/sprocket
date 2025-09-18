version 1.2

import "second.wdl"

workflow test {
    call second.test
}
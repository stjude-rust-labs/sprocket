version 1.4

import "first.wdl"

workflow test {
    call first.test
}

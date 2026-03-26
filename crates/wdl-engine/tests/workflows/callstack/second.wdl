version 1.3

import "first.wdl"

workflow test {
    call first.test
}

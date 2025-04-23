version 1.2

import "first.wdl"

workflow test {
    call first.test
}

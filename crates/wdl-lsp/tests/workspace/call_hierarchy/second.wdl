version 1.4

import "first.wdl"

workflow second {
    call first.first
}
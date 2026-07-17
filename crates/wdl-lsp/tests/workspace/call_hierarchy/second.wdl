version 1.3

import "first.wdl"

workflow second {
    call first.first
}
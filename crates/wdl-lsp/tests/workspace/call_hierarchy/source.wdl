version 1.3

import "first.wdl"
import "second.wdl"
import "third.wdl"

workflow all_together {
    call first.first {}
    call second.second {}
    call third.third {}
    call third.third as third2 {}
}
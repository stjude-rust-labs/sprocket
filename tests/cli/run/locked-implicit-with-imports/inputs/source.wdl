version 1.3

import "dep.wdl"

workflow do_work {
    call dep.print_release {}
}
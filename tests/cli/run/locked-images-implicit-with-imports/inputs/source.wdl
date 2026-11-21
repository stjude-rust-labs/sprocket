version 1.4

import "dep.wdl"

workflow do_work {
    call dep.print_release {}
}
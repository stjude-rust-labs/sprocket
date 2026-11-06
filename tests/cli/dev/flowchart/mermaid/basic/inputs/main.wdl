version 1.3

import "child.wdl" as child

task produce {
    command <<< echo value >>>
    output { String value = read_string(stdout()) }
}

task consume {
    input { String value }
    command <<< echo ~{value} >>>
}

workflow main {
    input {
        Boolean a
        Boolean b
    }

    call produce

    if (a || b) {
        call consume as either { input: value = produce.value }
    } else {
        call consume as neither { input: value = "none" }
    }

    scatter (item in ["x", "y"]) {
        call child.child { input: value = item }
    }

    call consume as last { input: value = select_first([child.out[0], produce.value]) }
}

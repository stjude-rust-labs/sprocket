version 1.3

import "one_level.wdl"

task inner {
    input {
        File bar
    }
    command <<<
        cat ~{bar} > out.txt
    >>>
    output {
        File bar_dup = "out.txt"
    }
}

workflow outer {
    input {
        #@ except: UnusedInput
        String foo
        # no bar!
    }
    call inner { }  # bar must be specified as a nested input
    call one_level.truncate_and_copy {}
    output {
        File duplicated = inner.bar_dup
        File truncated = truncate_and_copy.truncated
        File copied = truncate_and_copy.copied
    }

    hints {
        allow_nested_inputs: true
    }
}

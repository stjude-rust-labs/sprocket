version 1.3

import "two_levels.wdl"

task truncate {
    input {
        File to_truncate
    }
    command <<<
        head -c 50 ~{to_truncate} > out.txt
    >>>
    output {
        File truncated = "out.txt"
    }
}

# Two workflows deep
workflow truncate_and_copy {
    input {}

    call truncate { }
    call two_levels.copy_wf { }
    output {
        File truncated = truncate.truncated
        File copied = copy_wf.copied
    }

    hints {
        allow_nested_inputs: true
    }
}

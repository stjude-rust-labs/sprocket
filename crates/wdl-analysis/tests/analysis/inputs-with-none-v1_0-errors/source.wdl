## This is a test that fails to pass a required input without a
## default is still an error in WDL 1.0.
## See: https://github.com/stjude-rust-labs/sprocket/issues/812
#@ except: UnusedCall, UnusedDeclaration, UnusedInput
version 1.0

import "w.wdl"

task t {
    input {
        Int input1 = 1
        Int? input2 = 1
        Int? input3
        Int input4
    }

    command <<<
    >>>

    output {
        Int output1 = input1
        Int? output2 = input2
        Int? output3 = input3
        Int output4 = input4
    }
}

workflow test {
    input {
        Int? absent
    }

    call t as t1 { input:
        input1 = absent,
        input2 = absent,
        input3 = absent,
    }

    call t as t2

    call w.w as w1 { input:
        input1 = absent,
        input2 = absent,
        input3 = absent,
    }

    call w.w as w2
}

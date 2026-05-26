## This is a test that passing `None` to a required input without a
## default is still an error in WDL 1.2+.
## See: https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#optional-inputs-with-defaults

#@ except: UnusedInput, UnusedDeclaration, UnusedCall
version 1.3

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
    call t as t1 {
        input1 = None,
        input2 = None,
        input3 = None,
        input4 = None,
    }

    call t as t2 {
    }

    call w.w as w1 {
        input1 = None,
        input2 = None,
        input3 = None,
        input4 = None,
    }

    call w.w as w2 {
    }
}

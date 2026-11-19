## This is a test that inputs with default initializers retain their declared
## types in WDL 1.1 while callers may omit them or supply `None`.
## See: https://github.com/openwdl/wdl/pull/792
#@ except: UnusedCall, UnusedDeclaration, UnusedInput
version 1.1

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
        Int doubled = input1 * 2
    }
}

workflow test {
    input {
        Int? absent
    }

    call t as t1 { input:
        input1 = 42,
        input2 = 42,
        input3 = 42,
        input4 = 42,
    }

    call t as t2 { input:
        input1 = None,
        input2 = None,
        input3 = None,
        input4 = 42,
    }

    call t as t3 { input:
        input1 = absent,
        input2 = absent,
        input3 = absent,
        input4 = 42,
    }

    call t as t4 { input:
        input4 = 42,
    }

    call w.w as w1 { input:
        input1 = 42,
        input2 = 42,
        input3 = 42,
        input4 = 42,
    }

    call w.w as w2 { input:
        input1 = None,
        input2 = None,
        input3 = None,
        input4 = 42,
    }

    call w.w as w3 { input:
        input1 = absent,
        input2 = absent,
        input3 = absent,
        input4 = 42,
    }

    call w.w as w4 { input:
        input4 = 42,
    }

    output {
        Int t2_output1 = t2.output1
        Int? t2_output2 = t2.output2
        Int? t2_output3 = t2.output3
        Int t2_output4 = t2.output4
        Int t2_doubled = t2.doubled

        Int t3_output1 = t3.output1
        Int t4_output1 = t4.output1

        Int w2_output1 = w2.output1
        Int? w2_output2 = w2.output2
        Int? w2_output3 = w2.output3
        Int w2_output4 = w2.output4
        Int w2_doubled = w2.doubled

        Int w3_output1 = w3.output1
        Int w4_output1 = w4.output1
    }
}

## This is a test of the optional-to-default coercion in WDL 1.0.
## See: https://github.com/stjude-rust-labs/sprocket/issues/812

#@ except: UnusedInput, UnusedDeclaration, UnusedCall
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
    call t as t1 {
        input:
        input1 = 42,
        input2 = 42,
        input3 = 42,
        input4 = 42
    }

    call t as t2 {
        input:
        input1 = None,
        input2 = None,
        input3 = None,
        input4 = 42
    }

    call t as t3 {
        input:
        input4 = 42
    }

    call w.w as w1 {
        input:
        input1 = 42,
        input2 = 42,
        input3 = 42,
        input4 = 42
    }

    call w.w as w2 {
        input:
        input1 = None,
        input2 = None,
        input3 = None,
        input4 = 42
    }

    call w.w as w3 {
        input:
        input4 = 42
    }

    output {
        Int t1_output1 = t1.output1
        Int? t1_output2 = t1.output2
        Int? t1_output3 = t1.output3
        Int t1_output4 = t1.output4

        Int t2_output1 = t2.output1
        Int? t2_output2 = t2.output2
        Int? t2_output3 = t2.output3
        Int t2_output4 = t2.output4

        Int t3_output1 = t3.output1
        Int? t3_output2 = t3.output2
        Int? t3_output3 = t3.output3
        Int t3_output4 = t3.output4

        Int w1_output1 = w1.output1
        Int? w1_output2 = w1.output2
        Int? w1_output3 = w1.output3
        Int w1_output4 = w1.output4

        Int w2_output1 = w2.output1
        Int? w2_output2 = w2.output2
        Int? w2_output3 = w2.output3
        Int w2_output4 = w2.output4

        Int w3_output1 = w3.output1
        Int? w3_output2 = w3.output2
        Int? w3_output3 = w3.output3
        Int w3_output4 = w3.output4
    }
}

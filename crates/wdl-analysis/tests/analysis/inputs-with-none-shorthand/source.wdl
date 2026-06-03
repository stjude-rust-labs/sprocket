## This is a test of shorthand call inputs with optional values.
## See: https://github.com/stjude-rust-labs/sprocket/issues/812

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
    Int? input1 = 42
    Int? input2 = 42
    Int? input3 = 42
    Int input4 = 42

    call t as t1 { input1, input2, input3, input4 }

    call w.w as w1 { input1, input2, input3, input4 }

    output {
        Int t1_output1 = t1.output1
        Int? t1_output2 = t1.output2
        Int? t1_output3 = t1.output3
        Int t1_output4 = t1.output4

        Int w1_output1 = w1.output1
        Int? w1_output2 = w1.output2
        Int? w1_output3 = w1.output3
        Int w1_output4 = w1.output4
    }
}

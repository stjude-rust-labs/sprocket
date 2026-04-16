## This is a test that shorthand call inputs with optional values still
## error for required inputs without defaults.
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
    Int? input4 = 42

    call t as t1 { input1, input2, input3, input4 }

    call w.w as w1 { input1, input2, input3, input4 }
}

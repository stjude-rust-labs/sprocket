version 1.2

workflow w {
    input {
        Int input1 = 1
        Int? input2 = 1
        Int? input3
        Int input4
    }

    output {
        Int output1 = input1
        Int? output2 = input2
        Int? output3 = input3
        Int output4 = input4
    }
}

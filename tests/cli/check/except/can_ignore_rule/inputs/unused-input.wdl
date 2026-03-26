## This WDL file contains an unused input called x.
## This test intends to show check --except UnusedInput flag will cause it to ignore this rule, leading to no warnings being emitted

version 1.3

workflow test {
    input {
        Int x
    }
}
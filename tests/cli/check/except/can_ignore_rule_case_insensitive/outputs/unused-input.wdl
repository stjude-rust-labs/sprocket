## This WDL file contains an unused input called x.
## This test intends to show check --except UnUsEdInPuT flag will cause it to ignore this rule, leading to no warnings being emitted
## This test specifically shows that the rule flag being asked for still works even when the case does not match

version 1.2

workflow test {
    input {
        Int x
    }
}
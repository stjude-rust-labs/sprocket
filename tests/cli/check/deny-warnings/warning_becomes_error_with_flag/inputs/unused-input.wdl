## This WDL file contains an unused input called x.
## This test intends to show check --deny-warnings flag converts the unused input warning into an error instead

version 1.3


workflow test {
    input {
        Int x
    }
}
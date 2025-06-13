## This WDL file contains an unused input called x.
## This test intends to show check without any flags will properly show the warning for unused inputs

version 1.1


workflow test {
    input {
        Int x
    }
}
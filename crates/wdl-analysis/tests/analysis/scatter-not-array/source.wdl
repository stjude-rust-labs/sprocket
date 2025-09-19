## This is a test of attempting to scatter over something that is not an array.

version 1.1

workflow test {
    String a = "1"
    
    scatter (x in a) {

    }
}

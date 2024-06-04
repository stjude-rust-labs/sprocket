# This is a test for out of range integers.

version 1.1

workflow test {
    Int a = 0
    Int b = 9223372036854775807
    Int c = -9223372036854775807
    Int d = 0x8000000000000000
    Int e = 9223372036854775808 
    Int f = -9223372036854775808
    Int g = - 9223372036854775809
}

## This is a test to coerce elements of an array literal.
## See: https://github.com/stjude-rust-labs/wdl/issues/526
version 1.3

workflow test {
    Array[Int]+? a = [1, 2, 3]
    Array[Int] b = select_first([a, []])
}

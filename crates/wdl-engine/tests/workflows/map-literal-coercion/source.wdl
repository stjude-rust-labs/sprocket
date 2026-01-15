## This is a test to ensure coercion of a map literal's values.
## See: https://github.com/stjude-rust-labs/wdl/issues/526
version 1.3

workflow test {
    Array[Int]+? a = [1, 2, 3]
    Object b = object {
        key: { "a": a, "b": [] }
    }
}

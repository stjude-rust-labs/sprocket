## This is a test of evaluating an `if` expression where
## one side of the expression is an empty array.
version 1.1

workflow test {
    Array[Int] x = if false then [1, 2, 3] else []
}

## This is a test of accessing a pair.

version 1.1

task test {
    Pair[Int, Int] p = (1, 2)
    Int a = p.left
    Int b = p.right
    Int c = p.nope

    command<<<>>>
}

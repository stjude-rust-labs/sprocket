version 1.2

workflow map_to_array {
  Map[Int, Int] m = {0: 7, 1: 42}
  Array[Pair[Int, Int]] int_int_pairs = as_pairs(m)

  scatter (p in int_int_pairs) {
    Array[Int] a = [p.left, p.right]
  }

  output {
    Array[Array[Int]] aout = a
  }
}
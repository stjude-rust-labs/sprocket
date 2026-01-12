version 1.3

workflow test_map_ordering {
  # declaration using a map literal
  Map[Int, Int] int_to_int = { 2: 5, 1: 10 }

  scatter (ints in as_pairs(int_to_int)) {
    Array[Int] i = [ints.left, ints.right]
  }

  output {
    # evaluates to [[2, 5], [1, 10]]
    Array[Array[Int]] ints = i
  }
}
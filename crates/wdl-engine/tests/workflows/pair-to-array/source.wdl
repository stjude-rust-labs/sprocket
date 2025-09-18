version 1.2

workflow pair_to_array {
  Pair[Int, Int] p = (1, 2)
  Array[Int] a = [p.left, p.right]
  # We can convert back to Pair as needed
  Pair[Int, Int] p2 = (a[0], a[1])

  output {
    Array[Int] aout = a
  }
}
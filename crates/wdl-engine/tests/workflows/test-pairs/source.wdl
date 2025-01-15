version 1.2

workflow test_pairs {
  Pair[Int, Array[String]] data = (5, ["hello", "goodbye"])

  output {
    Int five = data.left  # evaluates to 5
    String hello = data.right[0]  # evaluates to "hello"
  }
}
version 1.3

workflow string_to_int_argument_fail {
  String count = "three"

  output {
    Array[Int] numbers = range(count)
  }
}

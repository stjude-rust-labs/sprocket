version 1.3

workflow string_to_int_array_fail {
  output {
    Array[Int] result = ["abc", 1]
  }
}

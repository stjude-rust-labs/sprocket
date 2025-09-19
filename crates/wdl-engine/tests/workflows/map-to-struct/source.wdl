version 1.2

struct IntStringMap {
  Array[Int] keys
  Array[String] values
}

workflow map_to_struct2 {
  Map[Int, String] m = {0: "a", 1: "b"}
  Array[Pair[Int, String]] int_string_pairs = as_pairs(m)
  Pair[Array[Int], Array[String]] int_string_arrays = unzip(int_string_pairs)

  IntStringMap s = IntStringMap {
    keys: int_string_arrays.left,
    values: int_string_arrays.right
  }

  # We can convert back to Map
  Map[Int, String] m2 = as_map(zip(s.keys, s.values))
  
  output {
    IntStringMap sout = s
    Boolean is_equal = m == m2
  }
}
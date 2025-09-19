version 1.2

workflow test_map_fail {
  Map[String, Int] string_to_int = { "a": 1, "b": 2 }
  Int c = string_to_int["c"]  # error - "c" is not a key in the map
}
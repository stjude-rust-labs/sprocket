version 1.3

workflow string_to_int_map_fail {
  output {
    Map[String, Int] result = { "x": "abc", "y": 1 }
  }
}

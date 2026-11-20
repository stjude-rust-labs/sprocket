version 1.3

struct Numbers {
  Int i
  Float f
  Boolean b
}

enum Mixed {
  One = 1,
  Two = " 2 "
}

task echo_int {
  input {
    Int n
  }

  command <<<>>>

  output {
    Int result = n
  }
}

workflow string_to_primitives {
  input {
    Int from_input
  }

  String two = "2"

  Int integer = " 42 "
  Float floating_point = " 3.5 "
  Float float_from_int = "7"
  Boolean boolean = " TRUE "
  Boolean false_boolean = "false"
  Int? optional = two
  Array[Int] array = ["1", " 2 ", "3"]
  Array[Int] mixed_array = [1, "2", 3]
  Array[Int] string_first_array = ["1", 2]
  Map[String, Int] string_first_map = { "a": "1", "b": 2 }
  Map[Int, Float] map = { "1": "1.5", "2": "2.5" }
  Numbers numbers = Numbers { i: "1", f: "2.5", b: "false" }
  Int if_common = if boolean then "10" else 20
  Array[Int] range_result = range(two)
  Boolean contains_result = contains([1, 2, 3], two)
  Boolean logical = "true" && boolean
  Int conditional = if ("TRUE") then 1 else 0
  String concat = two + "2"

  if (false_boolean) {
    Int merged = 1
  } else {
    String merged = "5"
  }

  call echo_int { n = two }

  output {
    Int integer_result = integer
    Float float_result = floating_point
    Float float_from_int_result = float_from_int
    Boolean boolean_result = boolean
    Boolean false_boolean_result = false_boolean
    Int? optional_result = optional
    Array[Int] array_result = array
    Array[Int] mixed_array_result = mixed_array
    Array[Int] string_first_array_result = string_first_array
    Map[String, Int] string_first_map_result = string_first_map
    Int merged_result = merged + 1
    Map[Int, Float] map_result = map
    Numbers numbers_result = numbers
    Int if_common_result = if_common
    Array[Int] range_result_output = range_result
    Boolean contains_result_output = contains_result
    Boolean logical_result = logical
    Int conditional_result = conditional
    String concat_result = concat
    Int enum_value = value(Mixed.Two)
    Int call_result = echo_int.result
    Int input_result = from_input
  }
}

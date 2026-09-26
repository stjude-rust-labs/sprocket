version 1.3

enum Mixed {
  One = 1,
  Two = "two"
}

workflow string_to_int_enum_fail {
  output {
    Int result = value(Mixed.Two)
  }
}

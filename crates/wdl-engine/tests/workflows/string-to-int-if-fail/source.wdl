version 1.3

workflow string_to_int_if_fail {
  Boolean condition = false

  output {
    Int result = if condition then 1 else "two"
  }
}

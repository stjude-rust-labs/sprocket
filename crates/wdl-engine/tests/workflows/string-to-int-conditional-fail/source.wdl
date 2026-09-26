version 1.3

workflow string_to_int_conditional_fail {
  Boolean condition = false

  if (condition) {
    Int x = 1
  } else {
    String x = "abc"
  }

  output {
    Int result = x + 1
  }
}

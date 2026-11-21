version 1.4

workflow test {
  input {
    Array[Int] values = [1, 2, 3]

    validation {
      # Fails: the condition is `false`
      length(values) > 5: "expected more than 5 values but found ~{length(values)}",
      # Fails: the condition fails to evaluate
      values[10] > 0: "this message is not evaluated",
      # Passes: the message would fail to evaluate but is not evaluated
      length(values) == 3: "~{values[20]}",
      # Fails: the message fails to evaluate
      false: "the value at index 7 is ~{values[7]}",
      # Passes
      values[0] == 1: "the first value must be 1",
      # Fails: the condition is `false`
      values[2] == 4: "the last value must be 4",
    }
  }
}

version 1.4

workflow workflow_output_validation_fail {
  input {
    Int value = 0
  }

  output {
    Int result = value

    validation {
      result > 0: "Result must be positive",
    }
  }
}

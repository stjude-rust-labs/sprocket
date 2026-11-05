version 1.4

workflow workflow_input_validation_fail {
  input {
    String name
    Int count

    validation {
      length(name) > 0: "Name must not be empty",
      count >= 1: "Count must be at least 1",
    }
  }
}

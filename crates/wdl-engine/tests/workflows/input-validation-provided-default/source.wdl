version 1.4

workflow test {
  input {
    Int minimum = 5
    Int value = minimum + 1

    validation {
      value >= minimum: "value ~{value} must be at least ~{minimum}",
    }
  }

  output {
    Int result = value
  }
}

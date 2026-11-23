version 1.4

task test {
  input {
    Int minimum = 5
    Int value = minimum + 1

    validation {
      value >= minimum: "value ~{value} must be at least ~{minimum}",
      value != 1: "value must not be 1",
    }
  }

  # This would fail if evaluated
  Int quotient = 1 / 0

  command <<<
  echo ~{quotient}
  >>>
}

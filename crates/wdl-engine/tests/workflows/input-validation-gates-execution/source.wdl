version 1.4

task fail {
  command <<<
  exit 1
  >>>
}

workflow test {
  input {
    Int n = 2

    validation {
      false: "the input validation gate is closed",
    }
  }

  # Each of the following would fail if evaluated
  Int quotient = 1 / 0

  call fail as top

  scatter (i in range(n)) {
    Int nested_quotient = i / 0

    if (i >= 0) {
      call fail as nested
    }
  }

  if (true) {
    call fail as conditional
  }

  output {
    Int result = quotient
  }
}

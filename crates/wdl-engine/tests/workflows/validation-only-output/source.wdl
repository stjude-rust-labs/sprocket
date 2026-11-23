version 1.4

task check {
  input {
    Int x
  }

  command <<<
  echo ~{x}
  >>>

  output {
    validation {
      read_int(stdout()) == x: "the task must echo its input",
    }
  }
}

workflow test {
  input {
    Int x = 1
  }

  call check { x }

  output {
    validation {
      x > 0: "x must be positive",
    }
  }
}

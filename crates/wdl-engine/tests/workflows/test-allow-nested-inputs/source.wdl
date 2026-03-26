version 1.3

task nested {
  input {
    String salutation
    String name = "Joe"
  }

  command <<<
  echo "~{salutation} ~{name}"
  >>>

  output {
    String greeting = read_string(stdout())
  }
}

workflow test_allow_nested_inputs {
  call nested {
    salutation = "Hello"
  }

  output {
    String greeting = nested.greeting
  }

  hints {
    allow_nested_inputs: true
  }
}
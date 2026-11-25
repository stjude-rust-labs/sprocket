version 1.2

task nested {
  input {
    ## Sample identifier
    String value
    String supplied
    Int count = 4
    File? reference
    String expression = value + "!"
  }
  command <<<>>>
}

workflow child {
  input {
    String fixed
  }
  call nested as inner {
    input: supplied = fixed
  }
  meta {
    allowNestedInputs: true
  }
}

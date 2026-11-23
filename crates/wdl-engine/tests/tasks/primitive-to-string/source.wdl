version 1.4

task primitive_to_string {
  input {
    Int i = 5
  }

  command <<<>>>

  output {
    String istring = "~{i}"
  }
}

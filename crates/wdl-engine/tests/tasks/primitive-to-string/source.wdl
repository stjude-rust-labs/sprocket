version 1.2

task primitive_to_string {
  input {
    Int i = 5
  }

  command <<<>>>

  output {
    String istring = "~{i}"
  }
}

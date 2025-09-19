version 1.2

workflow primitive_to_string {
  input {
    Int i = 5
  }

  output {
    String istring = "~{i}"
  }
}
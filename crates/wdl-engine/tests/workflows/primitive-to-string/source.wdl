version 1.3

workflow primitive_to_string {
  input {
    Int i = 5
  }

  output {
    String istring = "~{i}"
  }
}
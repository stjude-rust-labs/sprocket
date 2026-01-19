version 1.3

workflow nested_placeholders {
  input {
    Int i
    Boolean b
  }

  output {
    String s = "~{if b then '~{1 + i}' else '0'}"
  }
}
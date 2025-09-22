version 1.2

task nested_placeholders {
  input {
    Int i
    Boolean b
  }

  command <<<>>>

  output {
    String s = "~{if b then '~{1 + i}' else '0'}"
  }
}

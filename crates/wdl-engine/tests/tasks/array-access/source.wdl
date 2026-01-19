version 1.3

task array_access {
  input {
    Array[String] strings
    Int index
  }

  command <<<>>>

  output {
    String s = strings[index]
  }
}

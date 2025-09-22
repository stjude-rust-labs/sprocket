version 1.2

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

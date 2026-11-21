version 1.4

workflow array_access {
  input {
    Array[String] strings
    Int index
  }

  output {
    String s = strings[index]
  }
}
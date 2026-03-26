version 1.3

workflow array_access {
  input {
    Array[String] strings
    Int index
  }

  output {
    String s = strings[index]
  }
}
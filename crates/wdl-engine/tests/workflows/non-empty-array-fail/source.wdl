version 1.2

workflow non_empty_optional_fail {
  input {
    Array[Boolean] x
  }
  
  Array[Boolean]+ nonempty = x
}
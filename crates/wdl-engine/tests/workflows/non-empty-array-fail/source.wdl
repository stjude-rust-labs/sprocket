version 1.3

workflow non_empty_optional_fail {
  input {
    Array[Boolean] x
  }
  
  Array[Boolean]+ nonempty = x
}
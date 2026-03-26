version 1.3

workflow test_object {
  output {
    Object obj = object {
      a: 10,
      b: "hello"
    }
    Int i = obj.a
  }
}
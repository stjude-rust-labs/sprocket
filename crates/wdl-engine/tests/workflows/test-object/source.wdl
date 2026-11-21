version 1.4

workflow test_object {
  output {
    Object obj = object {
      a: 10,
      b: "hello"
    }
    Int i = obj.a
  }
}
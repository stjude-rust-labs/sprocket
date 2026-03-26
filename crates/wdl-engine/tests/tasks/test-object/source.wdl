version 1.3

task test_object {
  command <<<>>>

  output {
    Object obj = object {
      a: 10,
      b: "hello"
    }
    Int i = obj.a
  }
}

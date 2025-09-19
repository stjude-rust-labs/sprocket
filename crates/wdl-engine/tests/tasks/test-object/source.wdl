version 1.2

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

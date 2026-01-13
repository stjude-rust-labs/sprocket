version 1.3

struct MyType {
  String s
}

task foo {
  command <<<
  printf "bar"
  >>>

  MyType my = MyType { s: "hello" }

  output {
    String bar = read_string(stdout())
    String hello = my.s
  }
}

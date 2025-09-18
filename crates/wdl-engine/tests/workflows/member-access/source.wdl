version 1.2

struct MyType {
  String s
}

task foo {
  command <<<
  printf "bar"
  >>>

  output {
    String bar = read_string(stdout())
  }
}

workflow member_access {
  # task foo has an output y
  call foo
  MyType my = MyType { s: "hello" }

  output {
    String bar = foo.bar
    String hello = my.s
  }
}
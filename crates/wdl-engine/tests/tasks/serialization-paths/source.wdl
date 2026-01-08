## This is a test of properly translating host to guest paths during JSON serialization.
version 1.2

struct Foo {
  File file
}

task test {
  input {
    Foo foo
  }

  command <<<
    cat ~{write_json(foo)}
  >>>

  output {
    Foo out = read_json(stdout())
  }
}

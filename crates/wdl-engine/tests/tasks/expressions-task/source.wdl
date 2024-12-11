version 1.2

task expressions {
  input {
    Int x
  }

  command <<<
  printf "hello" > hello.txt
  >>>

  output {
    # simple expressions
    Float f = 1 + 2.2
    Boolean b = if 1 > 2 then true else false
    Map[String, Int] m = as_map(zip(["a", "b", "c"], [1, 2, 3]))

    # non-simple expressions
    Int i = x + 3  # requires knowing the value of x
    # requires reading a file that might only exist at runtime
    String s = read_string("hello.txt")
  }
}
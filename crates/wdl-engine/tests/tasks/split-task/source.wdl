version 1.3

task split {
  command <<<
    echo "hello there world" > file.txt
  >>>

  output {
    String out = read_string("file.txt")
    Array[String] parts = split(out, " ")
  }
}

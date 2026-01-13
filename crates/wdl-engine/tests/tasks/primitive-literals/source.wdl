version 1.3

task write_file_task {
  command <<<
  mkdir -p testdir
  printf "hello" > testdir/hello.txt
  >>>

  output {
    Boolean b = true 
    Int i = 0
    Float f = 27.3
    String s = "hello, world"
    File x = "testdir/hello.txt"
    Directory d = "testdir"
  }
}

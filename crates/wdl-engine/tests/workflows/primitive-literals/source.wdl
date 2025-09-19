version 1.2

task write_file_task {
  command <<<
  mkdir -p testdir
  printf "hello" > testdir/hello.txt
  >>>

  output {
    File x = "testdir/hello.txt"
    Directory d = "testdir"
  }
}

workflow primitive_literals {
  call write_file_task

  output {
    Boolean b = true 
    Int i = 0
    Float f = 27.3
    String s = "hello, world"
    File x = write_file_task.x
    Directory d = write_file_task.d
  }  
}
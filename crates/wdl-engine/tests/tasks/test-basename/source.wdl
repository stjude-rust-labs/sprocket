version 1.3

task test_basename {
  command <<<>>>

  output {
    Boolean is_true1 = basename("/path/to/file.txt") == "file.txt"
    Boolean is_true2 = basename("/path/to/file.txt", ".txt") == "file"
    Boolean is_true3 = basename("/path/to/dir") == "dir" 
  }
}
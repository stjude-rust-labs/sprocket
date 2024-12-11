version 1.2

task relative_and_absolute {
  command <<<
  mkdir -p my/path/to
  printf "something" > my/path/to/something.txt
  >>>

  output {
    String something = read_string("my/path/to/something.txt")
  }

  requirements {
    container: "ubuntu:focal"
  }
}

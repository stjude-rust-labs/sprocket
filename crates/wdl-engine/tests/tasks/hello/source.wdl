version 1.2

task hello_task {
  input {
    File infile
    String pattern
  }

  command <<<
    grep -E '~{pattern}' '~{infile}'
  >>>

  requirements {
    container: "ubuntu:latest"
  }

  output {
    Array[String] matches = read_lines(stdout())
  }
}

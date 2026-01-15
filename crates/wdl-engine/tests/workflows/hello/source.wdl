version 1.3

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

workflow hello {
  input {
    File infile
    String pattern
  }

  call hello_task {
    infile, pattern
  }

  output {
    Array[String] matches = hello_task.matches
  }
}
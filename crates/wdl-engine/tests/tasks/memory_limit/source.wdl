# Make sure the default configuration rejects an unsatisfiable memory requirement.

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
    memory: "20000 GiB"
  }

  output {
    Array[String] matches = read_lines(stdout())
  }
}

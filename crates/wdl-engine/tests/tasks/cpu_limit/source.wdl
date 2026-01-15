# Make sure the default configuration rejects an unsatisfiable cpu requirement.

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
    cpu: 4200
  }

  output {
    Array[String] matches = read_lines(stdout())
  }
}

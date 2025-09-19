version 1.2

task test_placeholders {
  input {
    File infile
  }

  command <<<
    # The `read_lines` function reads the lines from a file into an
    # array. The `sep` function concatenates the lines with a space
    # (" ") delimiter. The resulting string is then printed to stdout.
    printf "~{sep(" ", read_lines(infile))}"
  >>>
  
  output {
    # The `stdout` function returns a file with the contents of stdout.
    # The `read_string` function reads the entire file into a String.
    String result = read_string(stdout())
  }
}

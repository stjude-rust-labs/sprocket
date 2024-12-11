version 1.2

task echo_stdout {
  command <<< printf "hello world" >>>

  output {
    String message = read_string(stdout())
  }
}

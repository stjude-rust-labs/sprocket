version 1.4

task echo_stdout {
  command <<< printf "hello world" >>>

  output {
    String message = read_string(stdout())
  }
}

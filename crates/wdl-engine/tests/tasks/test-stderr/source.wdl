version 1.4

task echo_stderr {
  command <<< >&2 printf "hello world" >>>

  output {
    String message = read_string(stderr())
  }
}

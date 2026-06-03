version 1.2

task container_fallback {
  requirements {
    container: ["this-does-not-exist.invalid/fake:latest", "ubuntu:latest"]
  }

  command <<<
    uname -s
  >>>

  output {
    String os = read_string(stdout())
  }
}

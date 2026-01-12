version 1.3

task if_type_mismatch {
  command <<<
    echo '{ "hello": 1 }' >> file.txt
  >>>

  requirements {
    container: "ubuntu:latest"
  }

  output {
    File f = "file.txt"
    String out = if true then read_json(f) else "bar"
  }
}

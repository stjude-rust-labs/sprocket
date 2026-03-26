version 1.3

task change_extension {
  input {
    String prefix
  }

  command <<<
    printf "data" > '~{prefix}.data'
    printf "index" > '~{prefix}.index'
  >>>

  output {
    File data_file = "~{prefix}.data"
    String data = read_string(data_file)
    String index = read_string(sub(data_file, "\\.data$", ".index"))
  }

  requirements {
    container: "ubuntu:latest"
  }
}

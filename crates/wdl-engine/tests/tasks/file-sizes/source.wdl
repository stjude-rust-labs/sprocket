version 1.1

task file_sizes {
  command <<<
    printf "this file is 22 bytes\n" > created_file
  >>>

  File? missing_file = None

  output {
    Float missing_file_bytes = size(missing_file) # 0.0
    Float created_file_bytes = size("created_file", "B") # 22.0
    Float multi_file_kb = size(["created_file", missing_file], "K") # 0.022
  }

  runtime {
    container: "ubuntu:latest"
  }
}

version 1.3

task file_sizes {
  command <<<
    printf "this file is 22 bytes\n" > created_file
  >>>

  File? missing_file = None
  String? missing_path = None

  output {
    File created_file = "created_file"
    Float missing_file_bytes = size(missing_file)
    Float missing_path_bytes = size(missing_path, "B")
    Float created_file_bytes = size(created_file, "B")
    Float created_path_bytes = size("created_file", "B")
    Float multi_file_kb = size([created_file, missing_file], "K")
    Float nested_bytes = size({
      "a": (10, created_file),
      "b": (50, missing_file)
    })
  }
  
  requirements {
    container: "ubuntu:latest"
  }
}

version 1.3

workflow workflow_file_context {
  input {
    File source
  }

  String filename = "~{source}"
  File first_file = filename
  File literal_file = "data/hello.txt"
  Map[File, Int] file_map = {"data/hello.txt": 1}

  scatter (i in range(3)) {
    File scattered_file = filename
    Boolean same = scattered_file == first_file
  }

  output {
    Boolean same_files = first_file == source && literal_file == source
    Array[Boolean] scattered_same = same
    String contents = read_string(first_file)
    File output_file = "data/hello.txt"
    Boolean glob_same = glob("data/*.txt")[0] == literal_file
    Int map_value = file_map["data/hello.txt"]
    Boolean map_has_key = contains_key(file_map, "data/hello.txt")
    Boolean array_has_file = contains([literal_file], "data/hello.txt")
  }
}

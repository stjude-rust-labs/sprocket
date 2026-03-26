version 1.3

task string_to_file {
  String path1 = "data/file"
  File path2 = "data/file"

  # valid - String coerces unambiguously to File
  File path3 = path1

  command <<<>>>

  output {
    Boolean paths_equal = path2 == path3
  }
}

version 1.2

task string_to_file {
  String path1 = "/path/to/file"
  File path2 = "/path/to/file"

  # valid - String coerces unambiguously to File
  File path3 = path1

  command <<<>>>

  output {
    Boolean paths_equal = path2 == path3
  }
}

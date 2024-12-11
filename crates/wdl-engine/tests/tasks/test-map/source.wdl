version 1.2

task test_map {
  Map[Int, Int] int_to_int = {1: 10, 2: 11}
  Map[String, Int] string_to_int = { "a": 1, "b": 2 }
  Map[File, Array[Int]] file_to_ints = {
    "/path/to/file1": [0, 1, 2],
    "/path/to/file2": [9, 8, 7]
  }

  command <<<>>>

  output {
    Int ten = int_to_int[1]  # evaluates to 10
    Int b = string_to_int["b"]  # evaluates to 2
    Array[Int] ints = file_to_ints["/path/to/file1"]  # evaluates to [0, 1, 2]
  }
}

version 1.2

task array_map_equality {
  command <<<>>>

  output {
    # arrays and maps with the same elements in the same order are equal
    Boolean is_true1 = [1, 2, 3] == [1, 2, 3]
    Boolean is_true2 = {"a": 1, "b": 2} == {"a": 1, "b": 2}

    # arrays and maps with the same elements in different orders are not equal
    Boolean is_false1 = [1, 2, 3] == [2, 1, 3]
    Boolean is_false2 = {"a": 1, "b": 2} == {"b": 2, "a": 1}
  }
}

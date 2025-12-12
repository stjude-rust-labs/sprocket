version 1.3

enum Priority {
  Low = 1,
  Medium = 2,
  High = 3
}

workflow enum_in_compound_types {
  output {
    Array[Priority] priorities = [Priority.Low, Priority.Medium, Priority.High]
    Map[String, Priority] priority_map = {"task1": Priority.High, "task2": Priority.Low}
    Pair[Priority, Int] priority_pair = (Priority.Medium, 5)
    Priority first_priority = priorities[0]
    Priority task1_priority = priority_map["task1"]
    Priority pair_priority = priority_pair.left
    Int pair_count = priority_pair.right
  }
}

## This is a test from the 1.1 spec; the files do not have to exist for a 1.1 document
## This test will fail for 1.2 as it requires non-optional `File` values to be present
version 1.1

workflow test_as_map {
  input {
    Array[Pair[String, Int]] x = [("a", 1), ("c", 3), ("b", 2)]
    Array[Pair[String, Pair[File,File]]] y = [("a", ("a.bam", "a.bai")), ("b", ("b.bam", "b.bai"))]
    Map[String, Int] expected1 = {"a": 1, "c": 3, "b": 2}
    Map[String, Pair[File, File]] expected2 = {"a": ("a.bam", "a.bai"), "b": ("b.bam", "b.bai")}
  }

  output {
    Boolean is_true1 = as_map(x) == expected1
    Boolean is_true2 = as_map(y) == expected2
  }
}

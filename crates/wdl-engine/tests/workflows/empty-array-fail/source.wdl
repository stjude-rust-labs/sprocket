version 1.3

workflow empty_array_fail {
  Array[Int] empty = []
  
  output {
    # this causes an error - trying to access a non-existent array element
    Int i = empty[0]
  }
}
version 1.2

task non_empty_optional {
  command <<<>>>

  output {
    # array that must contain at least one Float
    Array[Float]+ nonempty1 = [0.0]
    # array that must contain at least one Int? (which may have an undefined value)
    Array[Int?]+ nonempty2 = [None, 1]
    # array that can be undefined or must contain at least one Int
    Array[Int]+? nonempty3 = None
    Array[Int]+? nonempty4 = [0]
  }
}

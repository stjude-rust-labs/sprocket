version 1.4

workflow illegal_access {
  input {
    Object my
  }

  Int i = my.x  # error: field 'x' does not exist in object
}
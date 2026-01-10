version 1.3

workflow illegal_access {
  input {
    Object my
  }

  Int i = my.x  # error: field 'x' does not exist in object
}
version 1.2

workflow illegal_access {
  input {
    Object my
  }

  Int i = my.x  # error: field 'x' does not exist in object
}
version 1.3

task declarations {
  input {
    # these "unbound" declarations are only allowed in the input section
    File? x  # optional - defaults to None
    Map[String, String] m  # required
    # this is a "bound" declaration
    String y = "abc"
  }

  Int i = 1 + 2  # Private declarations must be bound

  command <<<>>>

  output {
    Float pi = i + .14  # output declarations must also be bound
  }
}

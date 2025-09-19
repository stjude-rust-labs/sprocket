version 1.2

task say_hello {
  input {
    String greeting
  }

  command <<<
  printf "~{greeting}, how are you?"
  >>>

  output {
    String msg = read_string(stdout())
  }
}

workflow test_scatter {
  input {
    Array[String] name_array = ["Joe", "Bob", "Fred"]
    String salutation = "Hello"
  }
  
  # `name_array` is an identifier expression that evaluates to an Array 
  # of Strings.
  # `name` is a `String` declaration that is assigned a different value
  # - one of the elements of `name_array` - during each iteration.
  scatter (name in name_array) {
    # these statements are evaluated for each different value of `name`,s
    String greeting = "~{salutation} ~{name}"
    call say_hello { greeting = greeting }
  }

  output {
    Array[String] messages = say_hello.msg
  }
}
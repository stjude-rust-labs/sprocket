version 1.3

task greet {
  input {
    String greeting
  }

  command <<< printf "~{greeting}, nice to meet you!" >>>

  output {
    # expose the input to s as an output
    String greeting_out = greeting
    String msg = read_string(stdout())
  }
}

workflow copy_input {
  input {
    String name
  }

  call greet { greeting = "Hello ~{name}" }
  
  output {
    String greeting = greet.greeting_out
    String msg = greet.msg
  }
}
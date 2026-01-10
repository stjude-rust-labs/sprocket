version 1.3

task greet {
  input {
    String time
  }

  command <<<
  printf "Good ~{time} buddy!"
  >>>

  output {
    String greeting = read_string(stdout())
  }
}

workflow if_else {
  input {
    Boolean is_morning = false
  }
  
  # the body *is not* evaluated since 'b' is false
  if (is_morning) {
    call greet as morning { time = "morning" }
  }

  # the body *is* evaluated since !b is true
  if (!is_morning) {
    call greet as afternoon { time = "afternoon" }
  }

  output {
    String greeting = select_first([morning.greeting, afternoon.greeting])
  }
}
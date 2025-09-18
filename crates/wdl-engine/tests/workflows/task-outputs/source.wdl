version 1.2

task greet {
  input {
    String name
  }
  
  command <<<
    printf "Hello ~{name}"
  >>>

  output {
    String greeting = read_string(stdout())
  }
}

task count_lines {
  input {
    Array[String] array
  }

  command <<<
    wc -l '~{write_lines(array)}' | awk '{print $1}'
  >>>
  
  output {
    Int line_count = read_int(stdout())
  }
}

workflow task_outputs {
  call greet as x {
    name="John"
  }
  
  call greet as y {
    name="Sarah"
  }

  Array[String] greetings = [x.greeting, y.greeting]
  call count_lines {
    array=greetings
  }

  output {
    Int num_greetings = count_lines.line_count
  }
}
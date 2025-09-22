version 1.2

task test  {
  input {
    env String 
    greeting
  }

  command <<<
    echo $greeting
  >>>

  output {
    String out=read_string(stdout())
  }
}

workflow environment_variable_should_echo {
  input { String greeting
  }

  call test { 
    greeting }

  output {
    String out=test.out
  }
}

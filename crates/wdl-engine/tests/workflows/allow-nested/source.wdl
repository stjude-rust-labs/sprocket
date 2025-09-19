version 1.2

import "../call-example/source.wdl" as lib

task inc {
  input {
    Int y
    File ref_file # Do nothing with this
  }

  command <<<
  printf ~{y + 1}
  >>>

  output {
    Int incr = read_int(stdout())
  }
  
  requirements {
    container: "ubuntu:latest"
  }
}

workflow allow_nested {
  input {
    Int int_val
    String msg1
    String msg2
    Array[Int] my_ints
    File ref_file
  }

  hints {
    allow_nested_inputs: true
  }

  call lib.repeat {
    i = int_val,
    opt_string = msg1
  }

  call lib.repeat as repeat2 {
    # Note: the default value of `0` for the `i` input causes the task to fail
    opt_string = msg2
  }

  scatter (i in my_ints) {
    call inc {
      y=i, ref_file=ref_file
    }
  }

  output {
    Array[String] lines1 = repeat.lines
    Array[String] lines2 = repeat2.lines
    Array[Int] incrs = inc.incr
  }
}
version 1.2

import "../call-example/source.wdl" as lib

workflow test_after {
  # Call repeat
  call lib.repeat { i = 2, opt_string = "hello" }

  # Call `repeat` again with the output from the first call.
  # This call will wait until `repeat` is finished.
  call lib.repeat as repeat2 {
    i = 1,
    opt_string = sep(" ", repeat.lines)
  }

  # Call `repeat` again. This call does not depend on the output 
  # from an earlier call, but we specify explicitly that this 
  # task must wait until `repeat` is complete before executing.
  call lib.repeat as repeat3 after repeat { i = 3 }

  output {
    Array[String] lines1 = repeat.lines
    Array[String] lines2 = repeat2.lines
    Array[String] lines3 = repeat3.lines
  }
}
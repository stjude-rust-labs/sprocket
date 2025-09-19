version 1.2

import "../call-example/source.wdl" as lib

workflow test_input_keyword {
  input {
    Int i
  }

  # These three calls are equivalent
  call lib.repeat as rep1 { i }

  call lib.repeat as rep2 { i = i }

  call lib.repeat as rep3 {
    input:  # optional (for backward compatibility)
      i
  }

  call lib.repeat as rep4 {
    input:  # optional (for backward compatibility)
      i = i
  }

  output {
    Array[String] lines1 = rep1.lines
    Array[String] lines2 = rep2.lines
    Array[String] lines3 = rep3.lines
    Array[String] lines4 = rep4.lines
  }
}
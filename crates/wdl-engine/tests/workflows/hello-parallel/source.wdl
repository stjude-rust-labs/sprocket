version 1.2

import "../hello/source.wdl" as hello

workflow hello_parallel {
  input {
    Array[File] files
    String pattern
  }
  
  scatter (path in files) {
    call hello.hello_task {
      infile = path,
      pattern = pattern
    }
  }

  output {
    # WDL implicitly implements the 'gather' step, so the output of 
    # a scatter is always an array with the elements in the same 
    # order as the input array. Since hello_task.matches is an array,
    # all the results will be gathered into an array-of-arrays.
    Array[Array[String]] all_matches = hello_task.matches
  }
}
version 1.2

import "../test-local-call/source.wdl" as other_wf

task echo {
  input {
    String msg = "hello"
  }
  
  command <<<
  echo ~{msg}
  >>>
  
  output {
    File results = stdout()
  }
  
  requirements {
    container: "ubuntu:latest"
  }
}

workflow main {
  Array[String] arr = ["a", "b", "c"]

  call echo
  call echo as echo2
  call other_wf.foobar { infile = echo2.results }
  call other_wf.other { b = true, f = echo2.results }
  call other_wf.other as other2 { b = false }
  
  scatter(x in arr) {
    call echo as scattered_echo {
      msg = x
    }
    String scattered_echo_results = read_string(scattered_echo.results)
  }

  output {
    String echo_results = read_string(echo.results)
    Int foobar_results = foobar.results
    Array[String] echo_array = scattered_echo_results
  }
}
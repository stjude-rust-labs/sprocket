version 1.3

import "../input-ref-call/source.wdl" as ns1

workflow call_imported_task {
  input {
    Int x
    Int y = d1.out
  }

  call ns1.double as d1 { int_in = x }
  call ns1.double as d2 { int_in = y }

  output {
    Int result = d2.out
  }
}
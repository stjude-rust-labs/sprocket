version 1.3

task double {
  input {
    Int int_in
  }

  command <<< >>>

  output {
    Int out = int_in * 2
  }
}

workflow input_ref_call {
  input {
    Int x
    Int y = d1.out
  }

  call double as d1 { int_in = x }
  call double as d2 { int_in = y }

  output {
    Int result = d2.out
  }
}
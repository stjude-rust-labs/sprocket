version 1.2

import "../test-allow-nested-inputs/source.wdl" as test_allow_nested_inputs

workflow multi_nested_inputs { 
  call test_allow_nested_inputs.test_allow_nested_inputs

  hints {
    allow_nested_inputs: true
  }

  output {
    String greeting = test_allow_nested_inputs.greeting
  }
}
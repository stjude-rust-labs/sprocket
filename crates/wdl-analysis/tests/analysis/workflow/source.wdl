## This is an example taken from the WDL spec: https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#fully-qualified-names--namespaced-identifiers
## No diagnostics are expected

version 1.3

import "other.wdl" as lib

task repeat {
  input {
    Int i = 0  # this will cause the task to fail if not overridden by the caller
    String? opt_string
  }
  
  command <<<
  if [ "~{i}" -lt "1" ]; then
    echo "i must be >= 1"
    exit 1
  fi
  for i in 1..~{i}; do
    printf ~{select_first([opt_string, "default"])}
  done
  >>>

  output {
    Array[String] lines = read_lines(stdout())
  }
}

workflow call_example {
  input {
    String s
    Int i
  }

  # Calls repeat with one required input - it is okay to not
  # specify a value for repeat.opt_string since it is optional.
  call repeat { i = 3 }

  # Calls repeat a second time, this time with both inputs.
  # We need to give this one an alias to avoid name-collision.
  call repeat as repeat2 {
    i = i * 2,
    opt_string = s
  }

  # Calls repeat with one required input using the abbreviated 
  # syntax for `i`.
  call repeat as repeat3 { i, opt_string = s }

  # Calls a workflow imported from lib with no inputs.
  call lib.other
  # This call is also valid
  call lib.other as other_workflow2 {}

  output {
    Array[String] lines1 = repeat.lines
    Array[String] lines2 = repeat2.lines
    Array[String] lines3 = repeat3.lines
    Int? results1 = other.results
    Int? results2 = other_workflow2.results  
  }
}
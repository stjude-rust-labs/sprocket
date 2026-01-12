version 1.3

import "../if-else/source.wdl" as if_else

workflow nested_if {
  input {
    Boolean morning
    Boolean friendly
  }

  if (morning) {
    if (friendly) {
      call if_else.greet { time = "morning" }
    }
  }

  output {
    # Even though it's within a nested conditional, greeting
    # has a type of `String?` rather than `String??`
    String? greeting_maybe = greet.greeting

    # Similarly, `select_first` produces a `String`, not a `String?`
    String greeting = select_first([greet.greeting, "hi"])
  }
}
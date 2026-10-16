version 1.3

enum Type {
  A,
  B,
  C
}

task use_enum {
  input {
    Type? first = Type.A
    Type? second = Type.B
  }

  command <<< >>>

  output {
    Type? selected_first = first
    Type? selected_second = second
  }
}

workflow optional_enum_call_input {
  call use_enum {
    input:
      first = Type.B,
      second = Type.C
  }

  output {
    Type? selected_first = use_enum.selected_first
    Type? selected_second = use_enum.selected_second
  }
}

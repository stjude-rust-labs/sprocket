version 1.2

task flags {
  input {
    File infile
    String pattern
    Int? max_matches
  }

  command <<<
    # If `max_matches` is `None`, the command
    # grep -m ~{max_matches} ~{pattern} ~{infile}
    # would evaluate to
    # 'grep -m <pattern> <infile>', which would be an error.

    # Instead, make both the flag and the value conditional on `max_matches`
    # being defined.
    grep ~{"-m " + max_matches} ~{pattern} '~{infile}' | wc -l | sed 's/^ *//'
  >>>

  output {
    Int num_matches = read_int(stdout())
  }
}

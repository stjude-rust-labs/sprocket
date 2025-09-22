version 1.2

task multiline_strings {
  command <<<>>>

  output {
    String spaces = "  "
    String name = "Henry"
    String company = "Acme"
    # This string evaluates to: "  Hello Henry,\n  Welcome to Acme!"
    # The string still has spaces because the placeholders are evaluated after removing the 
    # common leading whitespace.
    String multi_line = <<<
      ~{spaces}Hello ~{name},
      ~{spaces}Welcome to ~{company}!
    >>>
  }
}

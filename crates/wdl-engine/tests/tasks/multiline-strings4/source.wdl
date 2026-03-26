version 1.3

task multiline_strings4 {
  command <<<>>>
  output {
    String multi_line_with_quotes = <<<
      multi-line string \
      with 'single' and "double" quotes
    >>>
  }
}

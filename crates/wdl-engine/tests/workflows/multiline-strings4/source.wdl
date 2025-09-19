version 1.2

workflow multiline_strings4 {
  output {
    String multi_line_with_quotes = <<<
      multi-line string \
      with 'single' and "double" quotes
    >>>
  }
}
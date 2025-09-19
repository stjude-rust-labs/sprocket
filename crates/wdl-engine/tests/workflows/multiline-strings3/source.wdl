version 1.2

workflow multiline_strings3 {
  output {
    # These strings are all equivalent. In strings B, C, and D, the middle lines are blank and 
    # so do not count towards the common leading whitespace determination.

    String multi_line_A = "\nthis is a\n\n  multi-line string\n"
    
    # This string's common leading whitespace is 0.
    String multi_line_B = <<<

    this is a
    
      multi-line string
    
    >>>

    # This string's common leading whitespace is 2. The middle blank line contains two spaces
    # that are also removed.
    String multi_line_C = <<<
    
      this is a
      
        multi-line string

    >>>
    
    # This string's common leading whitespace is 8.
    String multi_line_D = <<<

            this is a
    
              multi-line string

    >>>
  }
}
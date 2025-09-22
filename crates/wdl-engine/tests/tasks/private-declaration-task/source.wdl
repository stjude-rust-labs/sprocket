version 1.2

task private_declaration {
  input {
    Array[String] lines
  }

  Int num_lines = length(lines)
  Int num_lines_clamped = if num_lines > 3 then 3 else num_lines

  command <<<
  head -~{num_lines_clamped} '~{write_lines(lines)}'
  >>>

  output {
    Array[String] out_lines = read_lines(stdout())
  }
}

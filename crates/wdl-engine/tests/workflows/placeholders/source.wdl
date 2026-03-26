version 1.3

workflow placeholders {
  input {
    Int i = 3
    String start
    String end
    String instr
  }

  output {
    String s = "~{1 + i}"
    String cmd = "grep '~{start}...~{end}' ~{instr}"
  }
}
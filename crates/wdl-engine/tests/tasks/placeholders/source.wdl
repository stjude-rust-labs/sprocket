version 1.2

task placeholders {
  input {
    Int i = 3
    String start
    String end
    String instr
  }

  command <<<>>>

  output {
    String s = "~{1 + i}"
    String cmd = "grep '~{start}...~{end}' ~{instr}"
  }
}

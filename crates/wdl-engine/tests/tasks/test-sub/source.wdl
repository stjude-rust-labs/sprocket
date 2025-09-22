version 1.2

task test_sub {
  String chocolike = "I like chocolate when\nit's late"

  command <<<>>>

  output {
    String chocolove = sub(chocolike, "like", "love") # I love chocolate when\nit's late
    String chocoearly = sub(chocolike, "late", "early") # I like chocoearly when\nit's early
    String chocolate = sub(chocolike, "late$", "early") # I like chocolate when\nit's early
    String chocoearlylate = sub(chocolike, "[^ ]late", "early") # I like chocearly when\nit's late
    String choco4 = sub(chocolike, " [[:alpha:]]{4} ", " 4444 ") # I 4444 chocolate when\nit's late
    String no_newline = sub(chocolike, "\\n", " ") # "I like chocolate when it's late"
  }
}

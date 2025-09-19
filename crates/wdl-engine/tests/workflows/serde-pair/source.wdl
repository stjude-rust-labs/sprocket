version 1.1

task tail {
  input {
    Pair[File, Int] to_tail
  }

  command <<<
  tail -n ~{to_tail.right} '~{to_tail.left}'
  >>>

  output {
    Array[String] lines = read_lines(stdout())
  }
}

workflow serde_pair {
  input {
    Map[File, Int] to_tail
  }

  scatter (item in as_pairs(to_tail)) {
    call tail {
      input: to_tail = item
    }
    Pair[String, String]? two_lines = 
      if item.right >= 2 then (tail.lines[0], tail.lines[1]) else None
  }

  output {
    Map[String, String] tails_of_two = as_map(select_all(two_lines))
  }
}
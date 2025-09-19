version 1.2

task find_string {
  input {
    String in = "hello world"
    String pattern1 = "e..o"
    String pattern2 = "goodbye"
  }

  command <<<>>>

  output {
    String? match1 = find(in, pattern1)  # "ello"
    String? match2 = find(in, pattern2)  # None
  }
}

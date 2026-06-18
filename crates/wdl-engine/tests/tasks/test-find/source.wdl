version 1.3

task find_string {
  input {
    String text = "hello world"
    String pattern1 = "e..o"
    String pattern2 = "goodbye"
  }

  command <<<>>>

  output {
    String? match1 = find(text, pattern1)  # "ello"
    String? match2 = find(text, pattern2)  # None
  }
}

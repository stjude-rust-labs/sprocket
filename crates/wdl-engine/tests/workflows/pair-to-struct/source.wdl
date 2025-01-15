version 1.2

struct StringIntPair {
  String l
  Int r
}

workflow pair_to_struct {
  Pair[String, Int] p = ("hello", 42)
  StringIntPair s = StringIntPair {
    l: p.left,
    r: p.right
  }
  # We can convert back to Pair as needed
  Pair[String, Int] p2 = (s.l, s.r)

  output {
    StringIntPair sout = s
  }
}
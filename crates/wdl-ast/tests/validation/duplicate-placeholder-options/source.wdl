## This is a test of duplicate placeholder options.

version 1.2

task test {
  String a = "~{default="foo" foo}"
  String b = "${default="foo" sep="," foo}"
  String b = "${default="foo" sep="," true="a" false="b" foo}"

  command <<<
    ~{default="foo" foo}"
    ~{default="foo" sep="," foo}
    ~{default="foo" sep="," true="a" false="b" foo}
  >>>
}

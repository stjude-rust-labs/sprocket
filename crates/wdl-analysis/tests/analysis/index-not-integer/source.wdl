## This is a test of using a non-integer index.

version 1.1

task test {
    Array[String] a = ["foo", "bar", "baz"]
    String x = a["foo"]
    command <<<>>>
}

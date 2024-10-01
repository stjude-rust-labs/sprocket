## This is a test of using an invalid index for an array or map.

version 1.1

task test {
    Array[String] a = ["foo", "bar", "baz"]
    Map[String, String] b = {"foo": "bar", "baz": "qux"}
    String x = a["foo"]
    String y = b[0]
    command <<<>>>
}

## This is a test of coercing the result of an expression in a string interpolation.

version 1.1

task test {
    Array[String] a = ["foo", "bar"]

    # NOT OK
    String x = "foo ${a}"

    # OK
    String y = "foo ${sep="," a}"

    command <<<>>>
}

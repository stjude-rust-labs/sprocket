# This is a test of string interpolation.

version 1.1

task test {
    String name = 'world'
    String a = 'Hello ${name}'
    String b = "Hello ~{world}"
    String c = "Hello ${"world"}"
    String d = 'Hello ~{'world'}'
    String e = 'Hello ~{'to ${"you, ~{world}"}!'}'
    String f = "~{sep=" " [1, 2, 3]}"
    String g = "~{default="n/a" 1*2/2+1}"
    String h = "~{true="false" false="true" false}"
    String i = "~{sep('\n', [1, 2, 3])}" # Not a `sep` option
}

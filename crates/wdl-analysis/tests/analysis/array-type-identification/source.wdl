#@ except: UnusedDeclaration
## This is a simple test of identifying the inner type of an array.

version 1.2

task test {
    String? opt_string = "foo"
    Array[String?] x = [opt_string, "hi", 1]
    command <<<>>>
}

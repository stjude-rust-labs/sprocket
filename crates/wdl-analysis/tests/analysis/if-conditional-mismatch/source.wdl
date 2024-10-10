#@ except: UnusedDeclaration
## This is a test of a type mismatch in an if conditional.

version 1.1

task test {
    # BAD
    Int a = 1
    String b = if a then "foo" else "bar"

    # OK
    Boolean c = false
    String d = if c then "foo" else "bar"

    command <<<>>>
}

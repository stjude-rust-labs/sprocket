## This is a test of coercions from `String` to `Int`, `Float`, and `Boolean`.

#@ except: UnusedDeclaration, UnusedInput

version 1.3

struct Numbers {
    Int i
    Float f
    Boolean b
}

task foo {
    input {
        Int n
    }

    command <<<>>>
}

workflow test {
    String s = "1"

    # OK: `String` coerces to `Int`, `Float`, and `Boolean`
    Int i = " 42 "
    Float f = "3.5"
    Boolean b = "TRUE"
    Int? oi = s
    Array[Int] ai = ["1", "2"]
    Array[Int] mixed = [1, "2"]
    Map[Int, Float] m = { "1": "2.5" }
    Numbers numbers = Numbers { i: "1", f: "2.5", b: "false" }
    Int r = if true then 1 else "2"
    Array[Int] range_result = range("3")
    Boolean logical = "true" && b
    Int? conditional = if ("true") then 1 else None

    call foo { n = s }

    # OK: `String` concatenation is still `String`
    String concat = s + "2"
    String concat_int = s + 2

    # NOT OK: operators do not coerce `String` operands
    Float negated = -s
    Boolean compared = 1 == s
    Boolean less = s < 2
    Int indexed = ai[s]
}

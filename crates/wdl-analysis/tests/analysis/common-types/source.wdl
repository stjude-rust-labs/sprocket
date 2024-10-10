#@ except: UnusedDeclaration
## This tests calculations of common types.
## No diagnostics are expected from this test.

version 1.1

workflow test {
    File file = "test"

    # Tests for array literals
    Array[String?] a = ["foo", None, "bar"]
    Array[String?] b = [None, "foo", None]
    Array[String?] c = [None, None, None]
    Array[String?] d = [file, None]

    # Tests for map literals
    Map[String, String?] e = { "foo": "a", "bar": None, "baz": "c" }
    Map[String, String?] f = { "foo": None, "bar": "b", "baz": "c" }
    Map[String?, String] g = { "foo": "a", None: "b", "baz": "c" }
    Map[String?, String] h = { None: "a", "bar": "b", "baz": "c" }
    Map[String, String?] i = { "foo": None, "bar": file, "baz": "c" }
    Map[String?, String] j = { "foo": "a", None: "b", "baz": file }

    # Tests for `if` expressions
    String? k = if (true) then "foo" else None
    String? l = if (false) then None else "foo"
    String? m = if (false) then file else "foo"
    String? n = if (true) then "foo" else file
    String? o = if (false) then None else file
    String? p = if (true) then file else None
}

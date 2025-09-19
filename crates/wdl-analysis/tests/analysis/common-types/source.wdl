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

    # Tests for compound types
    Array[String?] q = ["foo", None, "baz"]
    Array[String?] r = ["foo", "bar", "baz"]
    Array[Pair[String?, Float]] s = [("foo", 1.0), (None, 2), ("baz", 3)]
    Map[String?, Float] t = { "foo": 1, None: 1.0 }
    Map[String?, String] u = { None: "bar", "foo": "baz" }
    Map[String?, Pair[Array[String?]?, Int]] v = { None: (["foo", None], 1), "foo": (None, 2) }
    Array[File]? w = ["foo"]
    Array[String] x = select_first([w, ["foo"]])
}

# This is a test for expression atoms.

version 1.1

task test {
    Int a = 1
    Float b = 3.14
    Boolean c = true
    Boolean d = false
    String e = 'hello'
    String f = "world"
    Array[Int] g = [a, 2, 3]
    Pair[Boolean, String] h = (true, "hi")
    Map[String, Array[String]] i = { 'hello': ['world'] }
    Object j = object { foo: 1, bar: "2", baz: 3.0 }
    MyStruct k = MyStruct { a: 1 + 2, b: "hello" + " world", c: c || d && c}
    Int l = g[0]
    Int m = if c then k + 1 else a * 2
    Array[Int] n = []
    Map[String, String] o = {}
    Foo p = Foo {}
    Object q = object {}
    String? r = None
    Boolean s = r == None
}

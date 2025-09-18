# This is a test of postfix expressions

version 1.1

task test {
    Int a = min(0, 1)
    Int b = min(max(100, a), 10)
    Array[String] c = ["a", "b", "c"]
    String d = c[a + b]
    MyStruct e = MyStruct {
        foo: MyFoo {
            bar: "baz",
        }
    }
    MyFoo f = MyFoo {
        foo: e.foo.bar
    }
}
#@ except: UnusedDeclaration
version 1.0

struct Inner {
    File? value
    File? other
}

struct Outer {
    Inner inner
    String? label
}

workflow reproduce {
    Inner inner = {"value": "example.txt"}
    Outer outer = {"inner": inner}
    Outer complete = {"inner": inner, "label": "x"}
    Outer invalid = {"inner": inner, "label": inner}
    Outer missing_required = {"label": "x"}
    Outer empty = {}

    output {
        File? value = outer.inner.value
        String? label = complete.label
    }
}

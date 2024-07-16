#@ except: DescriptionMissing, MissingMetas, NonmatchingOutput, SectionOrdering
## This is a test of the `DeprecatedObject` lint

version 1.1

workflow test {
    meta {}

    input {
        Object an_unbound_literal_object
    }

    Object a_bound_literal_object = object {
        a: 10,
        b: "foo",
    }

    output {
        Object another_bound_literal_object = object {
            bar: "baz"
        }

        #@ except: DeprecatedObject
        Object but_this_is_okay = object {
            quux: 42
        }
    }
}

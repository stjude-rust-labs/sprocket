version 1.1

struct Foo {
    String after
    String None
}

task foo {
    input {
        Int after
    }

    meta {
        # TODO: Still TBD on whether metadata sections should trigger this
        #       <https://github.com/openwdl/wdl/issues/763>
        None: "xyz"
    }

    command <<<>>>
}

version 1.0

struct Foo {
    String after
    String None
}

task foo {
    input {
        # `after` isn't reserved in 1.0
        Int after
    }

    meta {
        # TODO: Still TBD on whether metadata sections should trigger this
        #       <https://github.com/openwdl/wdl/issues/763>
        None: "xyz"
    }

    command <<<>>>
}

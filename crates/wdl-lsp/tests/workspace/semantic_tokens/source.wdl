#@ except: UnusedDeclaration, UnusedCall

version 1.3

import "foo.wdl"

struct Foo {
    Int bar
}

enum Hello {
    World,
}

task do_work {
    command <<<>>>
    
    output {
        Array[File] files = glob("*")
    }
}

## Documentation
workflow wf {
    call do_work
    
    call foo.bar

    Hello h = Hello.World
    Foo f = Foo {
        bar: 1
    }
}
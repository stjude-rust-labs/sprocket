#@ except: UnusedDeclaration, UnusedCall

version 1.3

import "foo.wdl" as bar

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
    
    call bar.bar

    Hello h = Hello.World
    Foo f = Foo {
        bar: 1
    }

    scatter (ignored in []) {}
}

task say_hello {
    meta {
        description: "Greet a person by name"
    }

    parameter_meta {
        name: "The name to greet"
    }

    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>

    output {
        Int? return_code = task.return_code
    }
}
#@ except: UnusedInput, UnusedDeclaration, UnusedCall
## This is a test of type checking `requirements` keys. 

version 1.2

struct Foo {
    String foo
    Int bar
}

task foo {
    command <<<>>>

    input {
        String foo
        Int bar
        Foo baz
    }

    output {
        String qux = "hi"
        Int quux = 1
        Foo corge = Foo { foo: "hi", bar: 1 }
    }

    hints {
        max_cpu: 1
        max_memory: 1
        disks: "1GiB"
        gpu: 1
        fpga: 1
        short_task: true
        localization_optional: false
        inputs: input {
            foo: hints {
                max_cpu: 1
            },
            bar: hints {
                max_cpu: 1
            },
            baz: hints {
                max_cpu: 1
            },
            baz.foo: hints {
                max_cpu: 1
            },
            baz.bar: hints {
                max_cpu: 1
            },
        }
        outputs: output {
            qux: hints {
                max_cpu: 1
            },
            quux: hints {
                max_cpu: 1
            },
            corge: hints {
                max_cpu: 1
            },
            corge.foo: hints {
                max_cpu: 1
            },
            corge.bar: hints {
                max_cpu: 1
            },
        }
        unsupported: false
    }
}

task bar {
    command <<<>>>

    input {
        String foo
        Int bar
        Foo baz
    }

    output {
        String qux = "hi"
        Int quux = 1
        Foo corge = Foo { foo: "hi", bar: 1 }
    }

    hints {
        maxCpu: 1.0
        maxMemory: "1"
        disks: { "foo": "1GiB" }
        gpu: "1"
        fpga: "1"
        shortTask: false
        localizationOptional: true
        unsupported: false
    }
}

task baz {
    command <<<>>>

    input {
        String foo
        Int bar
        Foo baz
    }

    output {
        String qux = "hi"
        Int quux = 1
        Foo corge = Foo { foo: "hi", bar: 1 }
    }

    hints {
        max_cpu: true
        max_memory: false
        disks: true
        gpu: false
        fpga: true
        short_task: "false"
        localization_optional: "true"
        inputs: input {
            wrong: hints {
                max_cpu: 1
            },
            baz.wrong: hints {
                max_cpu: 1
            },
            baz.foo.wrong: hints {
                max_cpu: 1
            },
            foo: "wrong"
        }
        outputs: output {
            wrong: hints {
                max_cpu: 1
            },
            corge.wrong: hints {
                max_cpu: 1
            },
            corge.foo.wrong: hints {
                max_cpu: 1
            },
            qux: "wrong"
        }
        unsupported: false
    }
}

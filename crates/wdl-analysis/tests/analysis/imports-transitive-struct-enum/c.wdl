#@ except: UnusedInput

version 1.3

import "b.wdl"

task foo {
    # We should be able to use types from a.wdl through b.wdl
    input {
        StructFromA my_struct = StructFromA { name: "John" }
        EnumFromA my_enum = EnumFromA.One
    }
    command <<<>>>
}

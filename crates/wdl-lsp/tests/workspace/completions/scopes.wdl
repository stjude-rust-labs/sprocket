version 1.4

import "lib.wdl" as lib

struct Person {
    String name
    Int age
}

workflow scopes {

}

task A {
    Int number = 1
    command <<<
    >>>

}

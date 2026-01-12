version 1.3

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

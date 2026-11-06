version 1.3

import "dep.wdl"
import "dep.wdl" as dep_alias

workflow do_work {
    call dep.say_hello
    call dep_alias.say_hello
}
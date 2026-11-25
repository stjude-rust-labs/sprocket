version 1.3

import "external.wdl"

struct LocalPerson {
    String name
}

enum LocalColor {
    Red,
    Blue
}

struct LocalPersonWrapper {
    LocalPerson person
}

workflow foo {
    LocalPerson local = LocalPerson { name: "John" }
    ExternalPerson external = ExternalPerson { name: "Joe" }
    LocalPerson? maybe_local = local
    LocalColor color = LocalColor.Red
    LocalPersonWrapper wrapper = LocalPersonWrapper { person: local }
    LocalPerson accessed = wrapper.person
    Pair[LocalPerson, Int] pair = (local, 1)
    LocalPerson from_pair = pair.left

    call external.greet_external {
        person = external
    }
    ExternalPerson from_call = greet_external.out

    Array[LocalPerson] people = [local]
    scatter (p in people) {
        String p_name = p.name
        LocalPerson scattered = p
    }
    Array[LocalPerson] escaped = scattered
}

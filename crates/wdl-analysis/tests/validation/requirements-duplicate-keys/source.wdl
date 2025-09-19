# This is a test for duplicate keys in a requirements section.

version 1.2

task test {
    requirements {
        container: "first"
        disks: "first"
        memory: "first"

        memory: "dup"
        container: "dup"
        disks: "dup"
    }

    command <<<>>>
}

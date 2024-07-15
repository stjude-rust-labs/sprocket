# This is a test for duplicate keys in a hints section.

version 1.2

task test {
    hints {
        container: "first"
        disks: "first"
        memory: "first"

        memory: "dup"
        container: "dup"
        disks: "dup"
    }

    command <<<>>>
}

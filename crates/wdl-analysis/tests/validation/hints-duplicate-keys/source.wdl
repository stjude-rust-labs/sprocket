# This is a test for duplicate keys in a hints section.

version 1.3

task test {
    command <<<>>>

    hints {
        container: "first"
        disks: "first"
        memory: "first"
        memory: "dup"
        container: "dup"
        disks: "dup"
    }
}

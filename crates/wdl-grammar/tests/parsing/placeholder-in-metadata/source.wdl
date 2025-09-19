# This is a test of a placeholder in a metadata value.

version 1.1

task test {
    meta {
        s: "this has a placeholder: ~{x}!"
    }
}

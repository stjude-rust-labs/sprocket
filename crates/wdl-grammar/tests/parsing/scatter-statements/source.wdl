# This is a test of scatter statements.

version 1.1

workflow test {
    scatter (a in [1, 2, 3]) {
        String msg = "hi"
        call x { input: a }
        call y { input: msg }
    }
}

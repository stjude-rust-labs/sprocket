## This is a test of using environment variable declarations in 1.2
## No diagnostics should be emitted

version 1.2

task test {
    input {
        String a
        env String b
    }

    String c = ""
    env String d = ""

    command <<<>>>
}

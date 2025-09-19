## This is a test of using environment variable declarations in < 1.2

version 1.1

task test {
    input {
        String a
        env String b
    }

    String c = ""
    env String d = ""

    command <<<>>>
}

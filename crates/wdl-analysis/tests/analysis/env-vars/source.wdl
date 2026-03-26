## This is a test of using task environment variables in WDL 1.2

version 1.3

task test {
    input {
        String a
        env String b
        env String c = ""
    }

    # This is unused because it is not referenced
    String d = ""

    # This is *not* unused because it is an environment variable,
    # regardless of whether or not it's referenced
    env String e = ""

    command <<<
        ~{a}
        ~{b}
        $c
    >>>
}

# This is a test of escaped newlines in string literals and commands.

version 1.1

task first {
    String x = "first \
second"

    command <<<
        this line has an escaped \
        newline
    >>>
}

task second {
    String x = 'first \
second'

    command {
        this line also has an escaped \
        newline
    }
}

# This is a test of string validation.
# Note that due an inexact path separator replacement in the tests,
# error messages in the baseline will show `/<escape>` instead of `\<escape>`.

version 1.1

task test {
    String a = "no problems here \\ \n \r \t \' \" \~ \$ \000 \777 \x00 \xfF \u0000 \uaAfF \U00000000 \UAaAaFfFf!"
    String b = "invalid escape sequence ~{"\j"}"
    String c = 'line \
                continuation'
    String d = "invalid ~{"octal"} here: \0"
    String e = "\xnn is an invalid hex escape!"
    String f = "this \u000 is too short"
    String g = 'this \UAAAXAAAA contains a non-hex character!'
    String h = "can't have a	tab!"
    String i = "can't have a
                newline"

    # For the command, only the string literal inside the placeholder
    # should cause an error
    command <<<
        no problems here \\ \n \r \t \' \" \~ \$ \000 \777 \x00 \xfF \u0000 \uaAfF \U00000000 \UAaAaFfFf!
        invalid escape sequence ~{"\j"}
        line \
        continuation
        invalid ~{"octal"} here: \0
        \xnn is an invalid hex escape!
        this \u000 is too short
        this \UAAAXAAAA contains a non-hex character!
        can have a	tab!
        can have a
            newline
    >>>
}

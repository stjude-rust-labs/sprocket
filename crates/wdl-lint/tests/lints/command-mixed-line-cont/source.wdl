## This is a test of having mixed indentation in a line continuation.

version 1.1

task test1 {
    command <<<
        this line has a continuation \
 		   and should be a warning
    >>>

    runtime {}
}

task test2 {
    command {
        this line has a continuation \
 		   and should be a warning
    }

    runtime {}
}

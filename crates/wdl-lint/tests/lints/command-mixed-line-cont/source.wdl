#@ except: DescriptionMissing, RuntimeSectionKeys

## This is a test of having mixed indentation in a line continuation.

version 1.1

task test1 {
    meta {}

    parameter_meta {}

    command <<<
        this line has a continuation \
 		   and should be a warning
    >>>

    output {}

    runtime {}
}

task test2 {
    meta {}

    parameter_meta {}

    #@ except: NoCurlyCommands
    command {
        this line has a continuation \
 		   and should be a warning
    }

    output {}

    runtime {}
}

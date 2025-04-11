#@ except: MetaDescription, ExpectedRuntimeKeys

## This is a test of having mixed _trailing_ indentation in command sections.
## There should be no warnings from the `CommandSectionIndentation` rule.

version 1.1

task test1 {
    meta {}

    parameter_meta {}

    command <<<
        this line is prefixed with ~{"spaces and has tailing mixed indentation"}  		
    >>>

    output {}

    runtime {}
}

task test2 {
    meta {}

    parameter_meta {}

    #@ except: HereDocCommands
    command {
        this line is prefixed with ${"spaces and has tailing mixed indentation"}  		
    }

    output {}

    runtime {}
}

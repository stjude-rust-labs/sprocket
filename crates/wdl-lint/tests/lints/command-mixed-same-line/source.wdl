#@ except: DescriptionMissing, RuntimeSectionKeys

## This is a test of having mixed indentation on the same line in command sections.

version 1.1

task test1 {
    meta {}

    parameter_meta {}

    command <<<
		    this line starts with tabs and ends with spaces
    >>>

    output {}

    runtime {}
}

task test2 {
    meta {}

    parameter_meta {}

    #@ except: NoCurlyCommands
    command {
    		this line starts with spaces and ends with tabs
    }

    output {}

    runtime {}
}

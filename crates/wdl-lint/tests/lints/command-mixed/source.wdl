#@ except: NoCurlyCommands
## This is a test of having mixed indentation on the same line in command sections.

version 1.1

task test1 {
    meta {}
    parameter_meta {}

    command <<<
		    this line has both tabs and spaces
    >>>

    output {}
    runtime {}
}

task test2 {
    meta {}
    parameter_meta {}

    command {
		    this line has both tabs and spaces
    }

    output {}
    runtime {}
}

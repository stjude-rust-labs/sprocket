## This is a test of having mixed indentation on the same line in command sections.

version 1.1

task test1 {
    command <<<
		    this line has both tabs and spaces
    >>>

    runtime {}
}

task test2 {
    command {
		    this line has both tabs and spaces
    }

    runtime {}
}

## This is a test of having spaces before tabs in command sections.

version 1.1

task test1 {
    command <<<
        this line is prefixed with spaces
		this line is prefixed with ~{"tabs"}
    >>>

    runtime {}
}

task test2 {
    command {
        this line is prefixed with spaces
		this line is prefixed with ~{"tabs"}
    }

    runtime {}
}

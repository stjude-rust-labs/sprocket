## This is a test of having tabs before spaces in command sections.

version 1.1

task test1 {
    command <<<
		this line is prefixed with ~{"tabs"}
        this line is prefixed with spaces
    >>>

    runtime {}
}

task test2 {
    command {
		this line is prefixed with ~{"tabs"}
        this line is prefixed with spaces
    }

    runtime {}
}

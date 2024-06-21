## This is a test of having tabs before spaces in command sections.

version 1.1

task test1 {
    meta {}
    parameter_meta {}

    command <<<
		this line is prefixed with ~{"tabs"}
        this line is prefixed with spaces
    >>>

    output {}
    runtime {}
}

task test2 {
    meta {}
    parameter_meta {}

    command {
		this line is prefixed with ~{"tabs"}
        this line is prefixed with spaces
    }

    output {}
    runtime {}
}

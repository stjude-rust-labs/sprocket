## This is a test of having spaces before tabs in command sections.

version 1.1

task test1 {
    meta {}
    parameter_meta {}

    command <<<
        this line is prefixed with spaces
		this line is prefixed with ~{"tabs"}
    >>>

    output {}
    runtime {}
}

task test2 {
    meta {}
    parameter_meta {}

    command {
        this line is prefixed with spaces
		this line is prefixed with ~{"tabs"}
    }

    output {}
    runtime {}
}

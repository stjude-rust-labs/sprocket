#@ except: DescriptionMissing, RuntimeSectionKeys

## This is a test of having mixed indentation inside of a placeholder.
## This should not cause a warning for the `CommandSectionMixedIndentation` rule.

version 1.1

task test1 {
    meta {}

    parameter_meta {}

    command <<<
        this line is ~{(
		    if true
		    then "split across multiple lines with mixed indentation"
		    else "by a placeholder"
	    )} but is all one literal line in the command text
    >>>

    output {}

    runtime {}
}

task test2 {
    meta {}

    parameter_meta {}

    #@ except: NoCurlyCommands
    command {
        this line is ${(
		    if true
		    then "split across multiple lines with mixed indentation"
		    else "by a placeholder"
	    )} but is all one literal line in the command text
    }

    output {}

    runtime {}
}

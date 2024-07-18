#@ except: DescriptionMissing, RuntimeSectionKeys

version 1.1

task task_a {
    meta {}
    parameter_meta {}
    input {}

    command <<<
        bin \
            this is going to be a really long line that is almost certainly going to be longer than the maximum line length I would hope \
            some other \
            this line is also going to be very very very long just to trip up our maximum line lint \
            shorter \
            options
    >>>

    output {}
    runtime {}
}

task task_b {
    meta {}
    parameter_meta {}
    input {}

    # This except currently causes the entire command block to fail.
    # That will be fixed in the future when we fix how excepting works.
    #@ except: LineWidth
    command <<<
        bin \
            this is going to be a really long line that is not going to trip up our maximum line lint because it is excepted \
            some other \
            this line is also going to be very very very long but it will also not trip up our maximum line line because it is excepted \
            shorter \
            options
    >>>

    output {}
    runtime {}
}

task task_c {
    meta {}
    parameter_meta {}
    input {}

    command <<< this is a task that has a very very very long command section on the first line. >>>

    output {}
    runtime {}
}

# Here is a very very very very very very very very very long comment that should absolutely eclipse 90 characters.

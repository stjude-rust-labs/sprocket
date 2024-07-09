#@ except: DescriptionMissing, MissingRuntime, MissingOutput

version 1.1

workflow foo {
    meta {}
    input {}
    output {}
    parameter_meta {}
    scatter (x in range(3)) {
        call bar
    }
    call baz
}

task bar {
    meta {}
    command <<< >>>
    parameter_meta {}
}

task baz {
    meta {}
    command <<< >>>
    output {}

}

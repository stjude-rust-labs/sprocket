#@ except: MissingRuntime, MissingOutput

version 1.1

task foo {
    meta {}
    command <<< >>>
}

task bar {
    meta {
        description: "this is a task"
    }
    command <<< >>>
}

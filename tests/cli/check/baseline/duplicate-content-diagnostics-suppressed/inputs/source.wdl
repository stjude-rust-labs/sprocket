version 1.3

task aa {
    meta {
        description: "task aa"
    }
    parameter_meta {
        x: "unused input"
    }
    input {
        Int x
    }
    command <<< >>>
    requirements {}
}

task bb {
    meta {
        description: "task bb"
    }
    parameter_meta {
        x: "unused input"
    }
    input {
        Int x
    }
    command <<< >>>
    requirements {}
}

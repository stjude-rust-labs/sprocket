version 1.2

#@ except: MissingRequirements, SnakeCase
task foo {
    meta {
        description: "This is a test of disallowed output names"
        outputs: {
            f: "not OK",
            outString: "not OK",
            output_string: "not OK",
            out_string: "not OK",
            outbound: "OK",
            outs: "OK",
        }
    }

    parameter_meta {
    }

    input {
    }

    command <<< >>>

    output {
        File f = "test.wdl"  # This is not OK
        String outString = "string"  # This is not OK
        String output_string = "string"  # This is not OK
        String out_string = "string"  # This is not OK
        String outbound = "string"  # This is OK
        Int outs = 1  # This is OK
    }

    runtime {}
}

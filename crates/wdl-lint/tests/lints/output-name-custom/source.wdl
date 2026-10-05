#@ except: BashSetSyntax

version 1.3

#@ except: DeprecatedRuntimeSection, RequirementsSection, SnakeCase
task foo {
    meta {
        description: "This is a test of the configurable output name checks"
        outputs: {
            f: "desc",
            ab: "desc",
            abc: "desc",
            abcd: "desc",
            abcde: "desc",
            outString: "desc",
            OutString: "desc",
            output_string: "desc",
            Output_string: "desc",
            out_string: "desc",
            outA: "desc",
            outbound: "desc",
        }
    }

    parameter_meta {}

    input {}

    command <<< >>>

    output {
        File f = "test.wdl"
        Int ab = 1
        Int abc = 1
        Int abcd = 1
        Int abcde = 1
        String outString = "string"
        String OutString = "string"
        String output_string = "string"
        String Output_string = "string"
        String out_string = "string"
        String outA = "string"
        String outbound = "string"
    }

    runtime {}
}

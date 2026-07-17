#@ except: BashSetSyntax

version 1.3

#@ except: EmptyOutputs, RequirementsSection
task foo {
    meta {
        description: "This is a test of disallowed input names"
    }

    parameter_meta {
        f: "Not OK"
        in_string_name: "Not OK"
        input_string: "Not OK"
        in_string: "Not OK"
        invalid: "OK"
    }

    input {
        File f  # This is not OK
        String in_string_name  # This is not OK
        String input_string  # This is not OK
        String in_string  # This is not OK
        String invalid  # This is OK
    }

    command <<<>>>

    output {}

    runtime {}
}

#@ except: DocMetaStrings

version 1.1

# Test workflow for ParameterDescription lint rule
workflow test_parameter_description{
    meta {
        description: "Test workflow for ParameterDescription rule"
        outputs: {
            # Valid: simple string description
            result: "The final result" ,
            # Valid: object with description key
            count: {
                description: "Number of items processed"
            },
            # INVALID: object without description key
            error_output: {
                help: "Some help text"
            },
            # INVALID: non-string, non-object value
            bad_output: 123,
        }
    }

    parameter_meta {
        # Valid: simple string description
        valid_input: "A valid input with description"
        # INVALID: non-string, non-object value
        missing_desc_input: 456
        # INVALID: object without description key
        flag_input: {
            help: "Some help for the flag",
            group: "options",
        }
        # Valid: object with description key
        number_input: {
            description: "A number to process",
            help: "Enter a positive integer",
        }
    }

    input {
        String valid_input
        String missing_desc_input
        Boolean flag_input
        Int number_input
    }

    output {
        String result = "final result value"
        Int count = 42
        String error_output = "error message"
        Int bad_output = 999
    }
}

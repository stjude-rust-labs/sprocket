#@ except: MatchingOutputMeta, MetaKeyValueFormatting, ExpectedRuntimeKeys
#@ except: ParameterMetaMatched, RuntimeSection, ParameterDescription

version 1.1

# Test workflow for DocMetaStrings lint rule
workflow test_expected_meta_string {
    meta {
        description: "This is a valid description"  # Valid string value - should pass
        help: "This is valid help text"  # Valid string value - should pass
        external_help: 12345  # Should warn: integer instead of string
        warning: true  # Should warn: boolean instead of string
        category: ["workflow", "test"]  # Should warn: array instead of string
        outputs: {
            result: {
                description: "The workflow result",  # Valid - should pass
                help: "This is the main output",  # Valid - should pass
                category: 123,  # Should warn: integer instead of string
            },
            description: 456,  # Should warn: integer instead of string
        }
    }

    parameter_meta {
        test_input: "A test input parameter"  # Valid simple string description - should pass
        number_input: {
            description: "A number input",
            help: "Enter a valid integer",
            group: "inputs",
        }  # Valid object with string values - should pass
        flag_input: 42  # Invalid: non-string value for parameter description - should warn
    }

    input {
        String test_input
        Boolean flag_input
        Int number_input
    }

    output {
        String result = "test completed"
    }
}

task test_task {
    meta {
        description: "Valid task description"
        warning: 999  # Should warn: integer instead of string
    }

    parameter_meta {
        test_input: {
            description: "Task input",
            help: null,  # Should warn: null instead of string
        }
    }

    input {
        String test_input
    }

    command <<<
        echo "~{test_input}"
    >>>

    output {
        String out = read_string(stdout())
    }
}

#@ except: RequirementsSection

version 1.2

# missing_outputs_key
task missing_outputs_key {
    meta {
        description: "Task without outputs key in meta"
    }

    command <<<
        echo "hello"
    >>>

    output {
        String result = read_string(stdout())
    }
}

# missing_specific_output
task missing_specific_output {
    meta {
        description: "Task missing one output from meta.outputs"
        outputs: {
            result: "The result string",
            # missing 'count' output
        }
    }

    command <<<
        echo "hello"
    >>>

    output {
        String result = read_string(stdout())
        Int count = 42
    }
}

# extra_output_key
task extra_output_key {
    meta {
        description: "Task with extra key in meta.outputs that doesn't exist"
        outputs: {
            result: "The result string",
            phantom: "This output doesn't actually exist",
        }
    }

    command <<<
        echo "hello"
    >>>

    output {
        String result = read_string(stdout())
    }
}

# out_of_order_keys
task out_of_order_keys {
    meta {
        description: "Task with outputs in wrong order"
        outputs: {
            second: "Second output",
            first: "First output",
        }
    }

    command <<<
        echo "hello"
    >>>

    output {
        String first = "a"
        String second = "b"
    }
}

# non_object_outputs
task non_object_outputs {
    meta {
        description: "Task with non-object meta.outputs"
        outputs: "This should be an object, not a string"
    }

    command <<<
        echo "hello"
    >>>

    output {
        String result = read_string(stdout())
    }
}

# correct_outputs
task correct_outputs {
    meta {
        description: "Task with correct meta.outputs"
        outputs: {
            result: "The result string",
            count: "The count value",
        }
    }

    command <<<
        echo "hello"
    >>>

    output {
        String result = read_string(stdout())
        Int count = 42
    }
}

# nested_meta_objects
task nested_meta_objects {
    meta {
        description: "Task with nested objects in meta"
        author: "test"
        outputs: {
            result: "The result string",
            details: {
                nested_key: "This is nested and should not be treated as an output",
            },
        }
        other_section: {
            some_key: "This should be ignored",
        }
    }

    command <<<
        echo "hello"
    >>>

    output {
        String result = read_string(stdout())
        String details = "info"
    }
}

# no_outputs
task no_outputs {
    meta {
        description: "Task without outputs section"
    }

    command <<<
        echo "hello"
    >>>
}

# meta_without_outputs_section
task meta_without_outputs_section {
    meta {
        description: "Task with meta but no output section"
        outputs: {
            result: "Documented but no output section",
        }
    }

    command <<<
        echo "hello"
    >>>
}

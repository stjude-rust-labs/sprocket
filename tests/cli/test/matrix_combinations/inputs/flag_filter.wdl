## copied from github.com/stjudecloud/workflows

version 1.1

struct FlagFilter {
    String include_if_all  # samtools -f
    String exclude_if_any  # samtools -F
    String include_if_any  # samtools --rf
    String exclude_if_all  # samtools -G
}

task validate_string_is_12bit_int {
    meta {
        description: "Validates that a string is a octal, decimal, or hexadecimal number and less than 2^12."
        help: "Hexadecimal numbers must be prefixed with '0x' and only contain the characters [0-9A-F] to be valid (i.e. [a-f] is not allowed). Octal number must start with '0' and only contain the characters [0-7] to be valid. And decimal numbers must start with a digit between 1-9 and only contain the characters [0-9] to be valid."
    }

    parameter_meta {
        number: "The number to validate. See task `meta.help` for accepted formats."
    }

    input {
        String number
    }

    command <<<
        if [[ "~{number}" =~ ^[1-9][0-9]*$ ]]; then
            # number is in decimal
            if [ "~{number}" -lt 4096 ]; then
                >&2 echo "Input number (~{number}) is valid"
            else
                >&2 echo "Input number (~{number}) interpreted as decimal"
                >&2 echo "But number must be less than 4096!"
                exit 42
            fi
        elif [[ "~{number}" =~ ^0[0-7]{0,4}$ ]] \
            || [[ "~{number}" =~ ^0x[0-9A-F]{1,3}$ ]]
        then
            # number is in octal or hexadecimal
            # and number is less than 4096(decimal)
            >&2 echo "Input number (~{number}) is valid"
        else
            # malformed for any reason
            >&2 echo "Input number (~{number}) is invalid"
            >&2 echo "See task description for valid formats"
            exit 42
        fi
    >>>

    runtime {
        container: "ghcr.io/stjudecloud/util:3.0.1"
        maxRetries: 1
    }
}

workflow validate_flag_filter {
    meta {
        name: "Validate FlagFilter"
        description: "Validates a FlagFilter struct."
    }

    parameter_meta {
        flags: "FlagFilter struct to validate"
    }

    input {
        FlagFilter flags
    }

    call validate_string_is_12bit_int as validate_include_if_any { input:
        number = flags.include_if_any
    }
    call validate_string_is_12bit_int as validate_include_if_all { input:
        number = flags.include_if_all
    }
    call validate_string_is_12bit_int as validate_exclude_if_any { input:
        number = flags.exclude_if_any
    }
    call validate_string_is_12bit_int as validate_exclude_if_all { input:
        number = flags.exclude_if_all
    }
}

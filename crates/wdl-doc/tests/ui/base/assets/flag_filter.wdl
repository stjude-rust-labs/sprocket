#@ except: UnusedCall

## # FlagFilter
##
## A struct to represent the filtering flags used in various `samtools` commands.
## The order of precedence is `include_if_all`, `exclude_if_any`, `include_if_any`,
## and `exclude_if_all`.
## These four fields correspond to the samtools flags
## `-f`, `-F`, `--rf`, and `-G` respectively.
## The values of these fields are strings that represent a 12bit bitwise flag.
## These strings must evaluate to an integer less than 4096 (2^12).
## They can be in octal, decimal, or hexadecimal format.
## Please see the `meta.help` of `validate_string_is_12bit_oct_dec_or_hex`
## for more information on the valid formats.
##
## The `validate_flag_filter` workflow can be used to validate a `FlagFilter` struct.
## **WARNING** The `validate_flag_filter` workflow will only check that all the fields
## can be parsed as integers less than 4096. It will not check if the flags are
## sensible input to `samtools fastq`.
## `samtools fastq` also employs very little error checking on the flags.
## So it is possible to pass in flags that produce nonsensical output.
## For example, it is possible to pass in flags that produce no output.
## **Please exhibit caution while modifying any default values of a `FlagFilter`.**
##
## We suggest using the Broad Institute's SAM flag explainer to construct the flags.
## Find it [here](https://broadinstitute.github.io/picard/explain-flags.html).
##
## ## Example input JSON
##
## ```json
## {
##    "flags": {
##        "include_if_all": "0x3",
##        "exclude_if_any": "0xF04",
##        "include_if_any": "0x0",
##        "exclude_if_all": "0x0"
##    }
## }
## ```
##
## ### Explanation
##
## The above example JSON represents a `FlagFilter` struct
## being passed to parameter named `flags`.
## The `include_if_all` field is set to `0x3` which is `3` in decimal.
## The `exclude_if_any` field is set to `0xF04` which is `3844` in decimal.
## The `include_if_any` field is set to `0x0` which is `0` in decimal.
## The `exclude_if_all` field is set to `0x0` which is `0` in decimal.
##
## `3` in decimal can be represented as `000000000011` in 12bit binary.
## This number means that to be included a read must have the 1st and 2nd bits set.
## Those bits correspond to the `read paired` and `read mapped in proper pair` flags.
##
## `3844` in decimal can be represented as `111100000100` in 12bit binary.
## This number means that to be excluded a read must have **any** of the
## 3rd, 9th, 10th, 11th, or 12th bits set.
## We won't go through what all those bits mean here, but you can find
## the meanings of the bits in the
## [SAM flag explainer](https://broadinstitute.github.io/picard/explain-flags.html).
## In short, those are all flags corresponding to the quality of the read
## and them being `true` may indicate that the read is of low quality and
## should be excluded.

version 1.1

struct FlagFilter {
    String include_if_all  # samtools -f
    String exclude_if_any  # samtools -F
    String include_if_any  # samtools --rf
    String exclude_if_all  # samtools -G
}

task validate_string_is_12bit_oct_dec_or_hex {
    meta {
        description: "Validates that a string is a octal, decimal, or hexadecimal number and less than 2^12."
        help: "Hexadecimal numbers must be prefixed with '0x' and only contain the characters [0-9A-F] to be valid (i.e. [a-f] is not allowed). Octal number must start with '0' and only contain the characters [0-7] to be valid. And decimal numbers must start with a digit between 1-9 and only contain the characters [0-9] to be valid."
        outputs: {
            check: "Dummy output to enable caching."
        }
    }

    parameter_meta {
        number: "The number to validate. See task `meta.help` for accepted formats."
    }

    input {
        String number
    }

    command <<<
        if [[ "~{number}" =~ ^[1-9][0-9]+$ ]]; then
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

    output {
        String check = "passed"
    }

    runtime {
        memory: "4 GB"
        disks: "10 GB"
        container: "ghcr.io/stjudecloud/util:1.4.0"
        maxRetries: 1
    }
}

workflow validate_flag_filter {
    meta {
        description: "Validates a FlagFilter struct."
        outputs: {
            check: "Dummy output to enable caching."
        }
    }

    parameter_meta {
        flags: "FlagFilter struct to validate"
    }

    input {
        FlagFilter flags
    }

    call validate_string_is_12bit_oct_dec_or_hex as validate_include_if_any { input:
        number = flags.include_if_any
    }
    call validate_string_is_12bit_oct_dec_or_hex as validate_include_if_all { input:
        number = flags.include_if_all
    }
    call validate_string_is_12bit_oct_dec_or_hex as validate_exclude_if_any { input:
        number = flags.exclude_if_any
    }
    call validate_string_is_12bit_oct_dec_or_hex as validate_exclude_if_all { input:
        number = flags.exclude_if_all
    }

    output {
        String check = "passed"
    }
}

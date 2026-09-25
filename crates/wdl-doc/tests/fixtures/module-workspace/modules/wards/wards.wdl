## Warding primitives for raw magical runes.
version 1.4

## Classification assigned after evaluating rune-level stability.
enum WardStatus {
    Stable,
    Flickering,
    Broken
}

## Summary metrics produced by the warding task.
struct WardMetrics {
    ## Total runes observed in the input file.
    Int total_runes

    ## Fraction of runes that held their ward.
    Float intact_fraction

    ## Overall warding classification.
    WardStatus status
}

## Calculates compact stability metrics for a rune file.
##
## The fixture uses fixed values so documentation can be generated
## without requiring executable fixtures.
task inspect_wards {
    input {
        ## Runes to inspect.
        File runes

        ## Minimum rune count required to pass.
        Int minimum_runes = 100000
    }

    command <<<
        printf '%s %s\n' '~{runes}' '~{minimum_runes}' > ward-inputs.txt
        printf '125000\n' > total_runes.txt
        printf '0.98\n' > intact_fraction.txt
    >>>

    output {
        ## Number of runes observed.
        Int total_runes = read_int("total_runes.txt")

        ## Fraction of runes that held their ward.
        Float intact_fraction = read_float("intact_fraction.txt")
    }

    requirements {
        container: "ubuntu:latest"
        cpu: 1
        memory: "1 GiB"
    }

    meta {
        description: "Computes rune-level warding metrics."
        outputs: {
            total_runes: "Total runes found in the input.",
            intact_fraction: "Fraction of runes that held their ward.",
        }
    }

    parameter_meta {
        runes: "Rune file to inspect."
        minimum_runes: "Minimum acceptable number of runes."
    }
}

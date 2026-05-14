#@ except: MetaSections, RequirementsSection, EmptyOutputs

version 1.3

task good {
    command <<<
        set -euo pipefail
        echo "Hello, World!"
    >>>
}

task good2 {
    command <<<
        set -eo pipefail
    >>>
}

task good3 {
    # The default config includes `nounset`
    command <<<
        set +u -eo pipefail
    >>>
}

task bad {
    command <<<
        set -uo pipefail
    >>>
}
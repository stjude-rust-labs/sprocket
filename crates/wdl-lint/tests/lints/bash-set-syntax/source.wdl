#@ except: MetaSections, RequirementsSection

version 1.3

task good {
    command <<<
        set -euo pipefail
        echo "Hello, World!"
    >>>
}

task good2 {
    # Extra flags are fine
    command <<<
        set -euCo pipefail
        echo "Hello, World!"
    >>>
}

task good3 {
    # Multiple commands are fine
    command <<<
        set -euo pipefail && echo "Hello, World!"
    >>>
}

task good4 {
    # All long options are fine
    command <<<
        set -o errexit -o nounset -o pipefail
        echo "Hello, World!"
    >>>
}

task good5 {
    command <<<
        # Hello
        # World
        # Incoming blank lines



        # And more comments
        set -euo pipefail
        echo "Hello, World!"
    >>>
}

task bad {
    command <<<
        echo "Hello, World!"
    >>>
}

task bad2 {
    # No -o pipefail
    command <<<
        set -eu
        echo "Hello, World!"
    >>>
}

task bad3 {
    # No -u
    command <<<
        set -eo pipefail
        echo "Hello, World!"
    >>>
}

task bad4 {
    command <<<
        set -eo pipefail && echo "Hello, World!"
    >>>
}

task bad5 {
    command <<<
        set
    >>>
}

task bad6 {
    # Explicitly *disabling* `e`
    command <<<
        set +e -uo pipefail
    >>>
}

task bad7 {
    # `set` must come first
    command <<<
        echo "Hello, World!"
        set -euo pipefail
    >>>
}

task bad8 {
    # `H` and `emacs` is interactive mode only
    command <<<
        set -euHo pipefail -o emacs
    >>>
}

task bad9 {
    # Making sure we don't freak out over bad -o usage
    command <<<
        set -euov pipefail
    >>>
}

task bad10 {
    # Or get fooled by non-set commands...
    command <<<
        setcap
    >>>
}
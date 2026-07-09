#@ except: MetaSections, RequirementsSection, EmptyOutputs, HereDocCommands

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

task good6 {
    # Arguments should be fine
    command <<<
        set -euo pipefail -- Hello
        echo "$1, World!"
    >>>
}

task good7 {
    # Should also check braced commands
    command {
        set -euo pipefail
        echo "Hello, World!"
    }
}

task good8 {
    # We only check the initial `set` command. Any overrides that happen
    # elsewhere in the block are up to the user to manage.
    # <https://github.com/stjude-rust-labs/sprocket/pull/843#discussion_r3249823828>
    command <<<
        set -euo pipefail
        set +e
    >>>
}

task good9 {
    # Should stop parsing on control characters
    command <<<
        set -euo pipefail;echo "Hello, world!"
    >>>
}

task good10 {
    command <<<
        set -euo pipefail&&echo "Hello, world!"
    >>>
}

task good11 {
    command <<<
        set -o pipefail -eu&&echo "Hello, world!"
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
    # Making sure we don't freak out over this bad -o usage
    command <<<
        set -euov pipefail
    >>>
}

task bad10 {
    # Or this one...
    command <<<
        set -euo
    >>>
}

task bad11 {
    # Or get fooled by non-set commands...
    command <<<
        setcap
    >>>
}

task bad12 {
    # Make sure we still flag bad commands with arguments
    command <<<
        set -o pipefail -- Hello
        echo "$1, World!"
    >>>
}

task bad13 {
    # Make sure we deny unknown flags
    command <<<
        set -euo pipefail -o hello_world -q
    >>>
}

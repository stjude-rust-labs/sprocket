#@ except: MetaSections, RequirementsSection, EmptyOutputs

version 1.4

task multiple_long_flags {
    # The config enforces multiple long-style flags. The suggested command should generate correctly.
    command <<<
        set -e
        echo "Hello, World!"
    >>>
}

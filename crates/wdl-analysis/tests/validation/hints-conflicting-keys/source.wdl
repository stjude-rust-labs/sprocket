## This is a test of conflicting `hints` keys.

version 1.3

task test {
    command <<<>>>

    # Check for conflicting keys
    hints {
        max_cpu: 1
        maxCpu: 1
        max_memory: 1
        maxMemory: 1
        localization_optional: true
        localizationOptional: true
    }
}

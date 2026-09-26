#@ except: BashSetSyntax, EmptyOutputs, RequirementsSection

version 1.3

task foo {
    meta {
        description: "longer than twenty characters"
    }

    command <<<>>>
}

task bar {
    meta {
        description: "short enough"
    }

    command <<<>>>
}

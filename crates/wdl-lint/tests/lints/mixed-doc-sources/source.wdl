#@ except: UnusedInput, MetaSections, EmptyOutputs, RequirementsSection, BashSetSyntax, DocMetaStrings, InputName, OutputName, MetaDescription, ParameterDescription

version 1.3

# No meta at all is fine
task good0 {
    command <<<>>>
}

## Just documented with comments
task good1 {
    command <<<>>>
}

task good2 {
    meta {
        description: "Just documented with a meta description"
    }

    command <<<>>>
}

## Documented with comments
task good3 {
    meta {
        extra: "But with extra meta"
    }

    command <<<>>>
}

task good4 {
    input {
        ## Input documented with comments
        String x
    }

    command <<<>>>
}

task good5 {
    input {
        String x
    }

    parameter_meta {
        x: "Input documented with parameter_meta"
    }

    command <<<>>>
}

task good6 {
    command <<<>>>

    output {
        ## Output documented with comments
        String x = ""
    }
}

task good7 {
    meta {
        outputs: {
            x: "Output documented with meta"
        }
    }

    command <<<>>>

    output {
        String x = ""
    }
}

## Sources are allowed to collide...
task good8 {
    meta {
        description: {
            rationale: "..if the meta isn't just a string literal"
        }
    }

    command <<<>>>
}

task good9 {
    input {
        ## Input sources can also collide...
        String x
    }

    parameter_meta {
        x: {
            rationale: "...for the same reason"
        }
    }

    command <<<>>>
}

task good10 {
    input {
        ## Input documented with comments
        String x
    }

    parameter_meta {
        x: {
            description: {
                rationale: "non-string description should not collide"
            }
        }
    }

    command <<<>>>
}

## Documented with comments
task bad1 {
    meta {
        description: "and a meta section?!"
    }

    command <<<>>>
}

## Documented with comments...
task bad2 {
    meta {
        help: "..that extend into the meta section"
    }

    command <<<>>>
}

task bad3 {
    input {
        ## Input documented with comments
        String x
    }

    parameter_meta {
        x: "and a meta section?!"
    }

    command <<<>>>
}

task bad4 {
    meta {
        outputs: {
            x: "Output documented with meta"
        }
    }

    command <<<>>>

    output {
        ## And a doc comment!
        String x = ""
    }
}

## Documented with comments
task bad5 {
    meta {
        description: "and description"
        help: "and help..."
    }

    command <<<>>>
}

task bad6 {
    input {
        ## Input documented with comments
        String x
    }

    parameter_meta {
        x: {
            description: "documented in object"
        }
    }

    command <<<>>>
}

struct Bad7 {
    ## Field documented with comments
    String x

    parameter_meta {
        x: "and a parameter_meta section?!"
    }
}
#@ except: UnusedInput, MetaSections, EmptyOutputs, RequirementsSection, BashSetSyntax, DocMetaStrings, InputName, OutputName, MetaDescription

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
    input {
        ## Input documented with comments
        String x
    }
    
    command <<<>>>
}

task good3 {
    command <<<>>>

    output {
        ## Output documented with comments
        String x = ""
    }
}

struct Good4 {
    ## Field documented with comments
    String x
}

task bad1 {
    meta {
        description: "Meta sections aren't allowed"
    }

    command <<<>>>
}

task bad2 {
    input {
        String x
    }

    parameter_meta {
        x: "parameter_meta isn't allowed"
    }

    command <<<>>>
}

task bad3 {
    meta {
        outputs: {
            x: "Meta sections aren't allowed"
        }
    }

    command <<<>>>

    output {
        String x = ""
    }
}

struct Bad4 {
    meta {
        description: "Meta sections aren't allowed"
    }

    String x
}

struct Bad5 {
    parameter_meta {
        x: "parameter_meta isn't allowed"
    }

    String x
}

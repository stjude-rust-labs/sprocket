#@ except: UnusedInput, MetaSections, EmptyOutputs, RequirementsSection, BashSetSyntax, DocMetaStrings, InputName, OutputName

version 1.3

# No meta at all is fine
task good0 {
    command <<<>>>
}

task good1 {
    meta {
        description: "Just documented with meta.description"
    }
    
    command <<<>>>
}

task good2 {
    input {
        String x
    }

    parameter_meta {
        x: "Input documented with parameter_meta"
    }
    
    command <<<>>>
}

task good3 {
    #@ except: MetaDescription
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

struct Good4 {
    meta {
        description: "Struct documented with meta"
    }

    String x
}

struct Good5 {
    parameter_meta {
        x: "Field documented with parameter_meta"
    }

    String x
}

## Doc comments aren't allowed
task bad1 {
    command <<<>>>
}

task bad2 {
    input {
        ## Doc comments aren't allowed
        String x
    }

    command <<<>>>
}

task bad3 {
    command <<<>>>

    output {
        ## Doc comments aren't allowed
        String x = ""
    }
}

struct Bad4 {
    ## Doc comments aren't allowed
    String x
}

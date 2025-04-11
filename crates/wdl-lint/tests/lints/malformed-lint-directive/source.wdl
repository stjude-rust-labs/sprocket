#@ except: CommentWhitespace, MetaDescription, RuntimeSection, KnownRules, LineWidth, PreambleCommentPlacement

## This is a test of the `LintDirectiveFormatted` rule

version 1.2

#@ stop: This should be flagged for using 'stop' instead of 'except'

#@ except: RequirementsSection
task foo {
    #@except: this should be flagged for missing a space
    meta {
    }

    command <<<>>>

    output {
    }

    runtime {
    }
}

workflow bar {
    meta {
    }

    #@ except this should be flagged for missing a colon
    output {
    }
}

struct Baz {  #@ except: this should be flagged for being inlined
    meta {
    }

    parameter_meta {
        x: "foo"
    }

    String x
}

workflow bar2 {
    meta {
    }

    #@     except: this should be flagged for excessive whitespace
    output {
    }
}

workflow bar3 {
    meta {
    }

    ## The following should be flagged for having a missing directive

    #@
    output {
    }
}

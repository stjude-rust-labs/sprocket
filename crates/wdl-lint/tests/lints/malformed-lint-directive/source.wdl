#@ except: CommentWhitespace, MetaDescription, RuntimeSection, KnownRules, LineWidth
#@ except: RequirementsSection, PreambleCommentPlacement

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

task bar2 {
    meta {
    }

    command <<<>>>

    #@     except: this should be flagged for excessive whitespace
    output {
    }
}

task bar3 {
    meta {
    }

    command <<<>>>

    ## The following should be flagged for having a missing directive

    #@
    output {
    }
}

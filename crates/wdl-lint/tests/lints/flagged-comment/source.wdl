#@ except: MetaDescription, MetaSections, RuntimeSection, EmptyOutputs, BashSetSyntax

version 1.1

# TODO: this should be flagged
# [TODO] this should be flagged


## TODO: This doc comment should not be flagged
#@ except: TODO Directives should not be flagged
workflow test {
    # This should be flagged (TODO).
    #@ except: FlaggedComment
    meta {
        # TODO: this should NOT be flagged
    }

    output {}
}

#@ except: FlaggedComment
task test2 {
    # TODO: This should NOT be flagged as well.
    command <<<>>>
}

#@ except: MetaDescription

## This is a test of the TodoComment rule.

version 1.1

# TODO: this should be flagged
# [TODO] this should be flagged

workflow test {
    # This should be flagged (TODO).
    #@ except: TodoComment
    meta {
        # TODO: this should NOT be flagged
    }

    output {}
}

#@ except: TodoComment
workflow test {
    # TODO: This should NOT be flagged as well.
    meta {}

    output {}
}

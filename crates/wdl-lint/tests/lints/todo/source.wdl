#@ except: DescriptionMissing

## This is a test of the Todo rule.

version 1.1

# TODO: this should be flagged
# [TODO] this should be flagged

workflow test {
    # This should be flagged (TODO).
    #@ except: Todo
    meta {
        # TODO: this should NOT be flagged
    }

    output {}
}

#@ except: Todo
workflow test {
    # TODO: This should NOT be flagged as well.
    meta {}

    output {}
}

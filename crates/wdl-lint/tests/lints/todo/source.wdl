#@ except: MetaDescription, MetaSections, OutputSection, RuntimeSection

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
task test2 {
    # TODO: This should NOT be flagged as well.
    command <<<>>>
}

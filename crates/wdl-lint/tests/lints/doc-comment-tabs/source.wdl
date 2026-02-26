#@ except: MetaDescription, MetaSections, RuntimeSection, ElementSpacing, TodoComment

version 1.1

##	This should be flagged (single tab)

##		This should produce ONE diagnostic (grouped tabs)

## Text	with	tab inside

## No tabs here (should NOT be flagged)

#	This is a normal comment with a tab (should NOT be flagged)

workflow test {

    ##	Workflow-level doc comment with tab (should be flagged)

    #@ except: DocCommentTabs
    meta {
        ##	This should NOT be flagged (local except works)
    }

    output {}
}

#@ except: DocCommentTabs
task test2 {

    ##	This should NOT be flagged either (document-level except for this task)

    command <<<>>>
}

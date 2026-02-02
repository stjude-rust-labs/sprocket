## This preamble should not be warned as it's not a doc comment!

version 1.3

## This doc comment is allowed
enum Color {
    ## As is this one
    Red,
    ## And so is this mutltiline
    ##
    ## one.
    Green

}

## This floating doc comment should be ignored by UnusedDocCommentsRule, but
## should get a lint by PreambleCommentPlacementRule.

#@ except: MetaSections
# Documented with comments
## A base person struct.
##
## This defines a named person.
struct Person {
    ## The name of the person.
    ##
    ## This is the full (first, last) name of the person.
    String name
}

#@ except: MetaSections
## This doc comment should be allowed
workflow test_workflow {
    ## This doc comment does nothing and the user should be warned!
    meta {
        description: "Show doc comments on bad placement of elements"
    }

    ## This input section can have a doc comment.
    input {
        Person person
    }

    ## As can the output
    output {}
}

#@ except: RequirementsSection
## This doc comment should be allowed
task test_task {
    meta {
        description: "Show doc comments on bad placement of elements"
    }

    ## You can't doc comment a comment - the user should be warned.
    # Comment about my doc comment
    ## Commands don't support doc comments so the user should be warned here.
    ## about this multiline comment with whitespace...
    ##
    ## ... that isn't doing anything!
    command <<<
        ## I'm a shell comment and shouldn't be picked up.
        printf "Hello World"
    >>>
}

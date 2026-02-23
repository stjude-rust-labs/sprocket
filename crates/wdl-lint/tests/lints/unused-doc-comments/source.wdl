## This preamble is considered a valid doc comment,
## despite having whitespace between it and the version statement.

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

## This doc comment should be allowed
#@ except: RequirementsSection
task test_task {
    meta {
        description: "Show doc comments on bad placement of elements"
    }

    ## The user should be warned about this doc comment as it targets
    ## command, despite the interruption below.
    # comment interrupting doc block resulting in two diagnostics
    ## Commands don't support doc comments so the user should be warned here.
    ## about this multiline comment with whitespace...
    ##
    ## ... that isn't doing anything!
    command <<<
        ## I'm a shell comment and shouldn't be picked up.
        printf "Hello World"
    >>>
}

#@ except: RequirementsSection, MetaSections
task test_task_2 {
    ## doc comment
    ## more text
    ## these 3 lines should all be highlighted by one span
    #@ directive
    # regular comment
    ## another doc comment block
    ## that gets a new diagnostic fired with a new span

    ## but there's a blank line in the middle!
    ## but we are going to consider these consecutive anyway
    ## this line and the prior 5 lines should all be in the same highlighted span
    # another regular comment for good measure

    command <<<>>>
}

## While it's not what we want people to do, I should be able to
## sandwich lint directives with doc comments or whitespace
## for the purposes of the unused doc comment lint.

#@ except: MetaSections, MatchingOutputMeta
## This doc comment should be allowed.
workflow test_workflow {
    ## This doc comment does nothing and the user should be warned!
    meta {
        description: "Show doc comments on bad placement of elements"
    }

    ## This input section cannot have a doc comment.
    input {
        ## But it's elements can.
        #@ except: UnusedInput
        Person person## Trailing doc comment without whitespace that should be linted.
        Boolean apple ## This doc comment should be linted, and should not be included in the block below it.
        ## A BoundDeclNode may have a doc comment if it's in an input section.
        Boolean banana = false
    }

    ## I am not allowed to be doc commented.
    call test_task {}

    ## You can't doc comment a BoundDeclNode if it's not within an Input or Output section.
    #@ except: UnusedDeclaration
    Person p = Person {
        name: "Brendon"
    }

    # Comments are definitely valid here.
    ## But doc comments are not!
    if (apple) {
        String favorite_fruit = "Apple"
    }
    # Comments seem fine here (although maybe a weird choice).
    ## But doc comments shouldn't be.
    else if (banana) {
        String favorite_fruit = "Banana"
    }
    # Seemingly, you can also put comments here,
    ## but we don't want doc comments here.
    else {
        ## Doc comments shouldn't be allowed on variable assignment!
        String favorite_fruit = "Chocolate"
    }

    ## The output section cannot have a doc comment!
    output {
        ## An element of an output should be doc commentable.
        Boolean my_output = banana
    }
}

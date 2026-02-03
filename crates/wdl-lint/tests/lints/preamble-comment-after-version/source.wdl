## This is a normal preamble comment.
       ## Bad leading whitespace!

version 1.1

## This is a preamble comment after the version

## Also a preamble comment

## And this one is bad too!

# This comment is okay though

### So is this comment

##### And this comment too!

## I'm not documenting the struct!

struct DetachedComment {
    String foo
}

## But I am!
struct AttachedComment {
    String foo
}

workflow test {
    ## This one is bad!
    #@ except: MetaDescription
    meta {}

    output {}
}

# This is an invalid preamble comment.
## This one is fine
# This one is invalid too!
### This one is invalid too!

version 1.1

workflow test {
    #@ except: MetaDescription
    meta {}

    parameter_meta {}

    output {}
}

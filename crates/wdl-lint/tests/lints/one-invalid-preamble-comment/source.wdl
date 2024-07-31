#@ except: BlankLinesBetweenElements, DescriptionMissing, PreambleWhitespace
# This is a test of having one big invalid preamble comment.
#

# All of the lines
# in this comment
# should be treated
#
#

# as a single diagnostic
# warning
## This last one is fine though!

version 1.1

workflow test {
    meta {}
    parameter_meta {}
    output {}
}

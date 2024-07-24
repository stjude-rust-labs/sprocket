#@ except: CommentWhitespace, DescriptionMissing
## This is a test of whitespace between import statements.

version 1.1

import "bar.wdl"    # OK

import "baz.wdl"    # BAD
    import "foo.wdl"   # BAD

import "huh.wdl"    # BAD
# but a comment makes it OK
import "vom.wdl"   # OK

# a comment and a blank is still BAD

import "wah.wdl"    # BAD


import "zam.wdl"    # 2 blanks will be caught be a _different_ check

workflow test {
    meta {}
    output {}
}

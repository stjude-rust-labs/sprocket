## This is a test of import whitespace.

version 1.1

import "foo.wdl"    # OK
import "bar.wdl"    # OK

import "baz.wdl"    # BAD

import "huh.wdl"    # BAD
# but a comment makes it OK
import "vom.wdl"   # OK

# a comment and a blank is still BAD

import "wah.wdl"    # BAD

workflow test {
    meta {}
    output {}
}

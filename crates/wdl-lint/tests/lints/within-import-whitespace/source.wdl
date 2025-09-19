#@ except: ElementSpacing, MetaDescription

## This is a test of whitespace within import statements and sort order.
## There should only ever be one diagnostic reported for a bad sort order.

version 1.1

import "foo.wdl" as foo  # OK
import  "bar.wdl"  # BAD (2 spaces)
import	"baz.wdl"  # BAD (tab literal)
import "chuk.wdl"        as something  # BAD (many spaces)
import "lorem.wdl" as 	ipsum  # BAD (space and tab)
import   "qux.wdl"  alias   Jabber    as    quux  # really BAD
import  # BAD (comment within statement)
"corge.wdl" as grault  # BAD (newline)

workflow test {
    meta {}
    output {}
}

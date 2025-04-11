#@ except: ElementSpacing, MetaDescription

## This is a test of whitespace within import statements and sort order.
## There should only ever be one diagnostic reported for a bad sort order.

version 1.1

import "foo" as foo  # OK
import  "bar"  # BAD (2 spaces)
import	"baz"  # BAD (tab literal)
import "chuk"        as something  # BAD (many spaces)
import "lorem" as 	ipsum  # BAD (space and tab)
import   "qux"  alias   jabber    as    quux  # really BAD
import  # BAD (comment within statement)
"corge" as grault  # BAD (newline)

workflow test {
    meta {}
    output {}
}

#@ except: UnusedImport
# This is a test of importing conflicting enum definitions.

version 1.4

import "foo.wdl"
import "bar.wdl"
import "baz.wdl"

workflow test {}

#@ except: UnusedImport
# This is a test of importing conflicting enum definitions.

version 1.3

import "foo.wdl"
import "bar.wdl"

workflow test {}

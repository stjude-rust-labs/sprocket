## This a test of importing by relative paths.
## There should be no diagnostics generated

#@ except: UnusedImport

version 1.1

import "a/file.wdl"

workflow test {}

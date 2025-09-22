#@ except: UnusedImport
## This a test of importing an identical struct.
## There should be no diagnostics generated

version 1.1

import "a/file.wdl" as a
import "b/file.wdl" as b

struct Foo {
    String a
    Int b
    File c
}

workflow test {
}

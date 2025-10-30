#@ except: UnusedImport
version 1.3

import "imported.wdl"

enum Status {
    Complete = 1,
    Failed = 2
}

workflow test {}

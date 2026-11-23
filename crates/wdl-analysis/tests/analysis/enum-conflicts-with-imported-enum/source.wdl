#@ except: UnusedImport
version 1.4

import "imported.wdl"

enum Status {
    Complete = 1,
    Failed = 2,
}

workflow test {}

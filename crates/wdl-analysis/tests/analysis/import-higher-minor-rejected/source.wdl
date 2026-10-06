# The minor-version import rule applies to every WDL version, not just 1.4+.
# A version 1.1 document importing a version 1.2 document is rejected because
# the imported minor version is higher than the importer's.
version 1.1

import "foo.wdl"

workflow test {}

# This is a test for an import with an unsupported version, but with a fallback configured. The
# imported document is interpreted as the fallback version (1.2) with a warning. Because 1.2 is a
# higher minor version than the importing document's 1.0, the import is additionally rejected as
# incompatible: an import must share the importer's major version and be no newer in minor version.

version 1.0

import "foo.wdl"

workflow test {}

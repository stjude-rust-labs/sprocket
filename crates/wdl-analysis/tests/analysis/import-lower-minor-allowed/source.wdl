#@ except: UnusedImport
# The counterpart to `import-higher-minor-rejected`: a version 1.2 document
# importing a version 1.1 document is allowed, since the imported minor
# version is not higher than the importer's. Confirms the rule is directional
# and applies to pre-1.4 versions.
version 1.2

import "foo.wdl"

workflow test {}

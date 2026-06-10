#@ except: UnusedImport
## This is a test of conflicting imports.

version 1.1

import "foo.wdl"                                    # First
import "foo"                                        # Conflicts
import "bad-file-name.wdl"                          # Bad name
import "foo" as bar                                 # First
import "Baz"                                        # First
import "qux/baz.wdl"                                # First
import "Baz.wdl" as baz                             # Conflicts
import "../conflicting-imports/qux/baz.wdl"         # Conflicts
import "md5sum.wdl"                                 # First
import "https://raw.githubusercontent.com/stjudecloud/workflows/efdca837bc35fe5647de6aa95989652a5a9648dc/tools/md5sum.wdl"            # Conflicts
import "https://raw.githubusercontent.com/stjudecloud/workflows/efdca837bc35fe5647de6aa95989652a5a9648dc/tools/md5sum.wdl#something"  # Conflicts
import "https://raw.githubusercontent.com/stjudecloud/workflows/d45a4c2a6d954b404db818f023b5a0453ac86fd0/tools/librarian.wdl?query=foo" # First
import "librarian.wdl"                                   # Conflicts
import "https://raw.githubusercontent.com/stjudecloud/workflows/d45a4c2a6d954b404db818f023b5a0453ac86fd0/tools/%6cibrarian.wdl" # Conflicts

workflow test {}

## This is a test of conflicting imports.

version 1.1

import "foo.wdl"                                    # First
import "foo"                                        # Conflicts
import "bad-file-name.wdl"                          # Bad name
import "foo" as bar                                 # First
import "Baz"                                        # First
import "BAZ"                                        # First
import "/baz.wdl"                                   # First
import "/Baz.wdl" as baz                            # Conflicts
import "../foo/bar/baz.wdl"                         # Conflicts
import "https://example.com/foo.wdl#something"      # Conflicts
import "https://example.com/qux.wdl?query=nope"     # First
import "qux.wdl"                                    # Conflicts
import "https://example.com/%66%6F%6F.wdl"          # Conflicts
import "https://example.com?query=foo"              # Bad name

workflow test {}

version 1.2

import "foo"

task foo {
}
# this is tricky, as the comment is at the end of the file
# but it will be encountered during processing of the above import
# statement (which is the first thing written after the version).

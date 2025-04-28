## This is a test of having a placeholder in an import

version 1.1

import "this contains ~{"a placeholder"}" as foo
import "this also contains ${"a placeholder"}" as bar
import "ok.wdl" as baz

workflow test {

}

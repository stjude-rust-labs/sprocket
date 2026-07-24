version 1.4

import
openwdl/very/long/nested/module/path/that/definitely/exceeds/the/max/line/length/csvkit # interrupted
/and/continued/on/new/line
as csv
import { Foo } from openwdl/very/long/nested/module/path/that/definitely/exceeds/the/max/line/length/csvkit/                    # interrupted by very weird whitespace
          and/continued/on/new/line # and then some more weirdo space
workflow test {}

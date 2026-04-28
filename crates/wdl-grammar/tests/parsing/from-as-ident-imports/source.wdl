version 1.4

import "lib.wdl" as from
import "lib.wdl" alias from as MyFrom
import "lib.wdl" alias Bar as from
import { from } from "csvkit.wdl"
import { Foo as from } from "csvkit.wdl"
import openwdl/from/csvkit

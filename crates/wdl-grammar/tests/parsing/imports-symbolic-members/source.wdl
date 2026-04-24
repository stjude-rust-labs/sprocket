version 1.4

# Single member, without and with module alias.
import { sort } from openwdl/csvkit
import { sort } from openwdl/csvkit as csv

# Renamed member, without and with module alias.
import { sort as sorter } from openwdl/csvkit
import { sort as sorter } from openwdl/csvkit as csv

# Dotted (namespaced) member, without and with module alias.
import { sort.CsvSort } from openwdl/csvkit
import { sort.CsvSort } from openwdl/csvkit as csv

# Dotted member with rename, without and with module alias.
import { sort.CsvSort as MySort } from openwdl/csvkit
import { sort.CsvSort as MySort } from openwdl/csvkit as csv

# Multiple members mixing shapes, without and with module alias.
import { sort.CsvSort, sort.CsvSortStable as Stable, cut } from openwdl/csvkit
import { sort.CsvSort, sort.CsvSortStable as Stable, cut } from openwdl/csvkit as csv

# Trailing comma.
import { sort, } from openwdl/csvkit

# Deeply nested dotted path.
import { a.b.c.deep, ns.inner.Name as Renamed } from openwdl/csvkit

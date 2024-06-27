#@ except: EndingNewline,PreambleWhitespace,MissingMetas,MissingOutput
## This is a test of the `InconsistentNewlines` lint
## Note that due an inexact path separator replacement in the tests,
## error messages in the baseline will show `/<escape>` instead of `\<escape>`.

version 1.1

workflow foo {}



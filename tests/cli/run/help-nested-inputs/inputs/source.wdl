version 1.2

import "child.wdl" as imported

workflow wf {
  call imported.child as outer {
    input: fixed = "provided"
  }
  meta {
    allowNestedInputs: true
  }
}

version 1.4

import "https://raw.githubusercontent.com/stjudecloud/workflows/efbec4e6ef3afaac17a9a18021afb08ce2debbb3/tools/util.wdl"
import { module_task } from local_module
import { FqRelease } from external_module

workflow foo {
    call util.split_string {
        string = "hello,world",
        delimiter = ","
    }

    call module_task

    FqRelease release = FqRelease {
        tool_version: "1"
    }
}
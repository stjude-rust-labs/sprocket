version 1.4

import "https://raw.githubusercontent.com/stjudecloud/workflows/d45a4c2a6d954b404db818f023b5a0453ac86fd0/tools/librarian.wdl"
import "foo.wdl"
import "./foo.wdl" as foo2
import { say_hello } from "foo.wdl"
import module

workflow dummy {}
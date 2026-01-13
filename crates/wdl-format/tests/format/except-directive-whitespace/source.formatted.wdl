version 1.3

#@ except: ARule, ZRule
task test1 {
}

#@ except: Bar, Foo
task test2 {
}

#@ except: Bar, Foo
task test3 {
}

#@ except: Bar, Foo, Zulu
task test4 {
}

#@ except: End, Middle, NoSpace
task test5 {
}

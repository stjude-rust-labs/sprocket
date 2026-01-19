version 1.3

#@    except:ZRule,ARule
task test1 {}

#@ except:   Foo ,Bar
task test2 {}

#@ except:Foo,   Bar
task test3 {}

#@ except:  Foo ,   Bar ,   Zulu
task test4 {}

#@except:NoSpace,Middle,End
task test5 {}

## This is a test of using a `None` in a `Map` literal
version 1.3

task test {
    #@except: UnusedDeclaration
    Map[String?, String] incorrect = { None: "wrong" }
    
    command <<<>>>
}

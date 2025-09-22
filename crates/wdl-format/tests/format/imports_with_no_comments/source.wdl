            version 1.1
    
            import "fileB.wdl" as foo
            import "fileA.wdl" as bar alias cows as horses
            alias cats as dogs
            workflow test {}
            import "fileC.wdl" alias qux as Qux

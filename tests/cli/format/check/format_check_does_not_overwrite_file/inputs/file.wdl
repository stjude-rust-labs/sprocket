## This file is a poorly formatted WDL file
## The intention of this test is to show how the format --check command will just output 
## how it would format the WDL file in stdout without overwriting the file

    version 1.3


workflow test {
                            input {
            Int x
        }
}
#@ except: Foo, UnknownRule
     
## The above line has extra whitespace
## This is a test of lines that only contain whitespace
## The next line has spaces
          
version 1.1

# The next line only contains whitespace
	
# The next has multiple blank lines in a row

          


workflow test {    
    # lines above and below have trailing whitespace
    #@ except: DescriptionMissing        
    meta {}
    
    parameter_meta {}

    String x = ""           

    output {}
}
     

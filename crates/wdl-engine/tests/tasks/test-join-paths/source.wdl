version 1.2

task resolve_paths_task {
  input {
    File abs_file = "/usr"
    String abs_str = "/usr"
    String rel_dir_str = "bin"
    File rel_file = "echo"
    File rel_dir_file = "mydir"
    String rel_str = "mydata.txt"
  }

  # these are all equivalent to '/usr/bin/echo'
  File bin1 = join_paths(abs_file, [rel_dir_str, rel_file])
  File bin2 = join_paths(abs_str, [rel_dir_str, rel_file])
  File bin3 = join_paths([abs_str, rel_dir_str, rel_file])
  
  # the default behavior is that this resolves to 
  # '<working dir>/mydir/mydata.txt'
  File data = join_paths(rel_dir_file, rel_str)
  
  # this resolves to '<working dir>/bin/echo', which is non-existent
  File doesnt_exist = join_paths([rel_dir_str, rel_file])
  command <<<
    mkdir '~{rel_dir_file}'
    echo -n "hello" > '~{data}'
  >>>

  output {
    Boolean bins_equal = (bin1 == bin2) && (bin1 == bin3)
    String result = read_string(data)
    File? missing_file = doesnt_exist
  }
  
  runtime {
    container: "ubuntu:latest"
  }
}
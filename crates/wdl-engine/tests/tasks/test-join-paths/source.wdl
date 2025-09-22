version 1.2

task resolve_paths_task {
  # We use strings here as the paths might not exist
  String abs_file = "/usr"
  String abs_str = "/usr"
  String rel_dir_str = "bin"
  String rel_file = "echo"
  String rel_dir_file = "mydir"
  String rel_str = "mydata.txt"
  
  # These are all equivalent to '/usr/bin/echo'
  String bin1 = join_paths(abs_file, [rel_dir_str, rel_file])
  String bin2 = join_paths(abs_str, [rel_dir_str, rel_file])
  String bin3 = join_paths([abs_str, rel_dir_str, rel_file])
  
  # This resolves to mydir/mydata.txt (which exists)
  File data = join_paths(rel_dir_file, ["..", "mydata.txt"])
  
  # this resolves to 'bin/echo', which is non-existent
  File? doesnt_exist = join_paths([rel_dir_str, rel_file])
  command <<<
    cat '~{data}'
  >>>

  output {
    Boolean bins_equal = (bin1 == bin2) && (bin1 == bin3)
    String result = read_string(stdout())
    File? missing_file = doesnt_exist
  }
  
  runtime {
    container: "ubuntu:latest"
  }
}
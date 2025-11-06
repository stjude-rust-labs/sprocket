version 1.3

struct NetworkConfig {
  String hostname
  Int port
  Boolean tls
}

struct ServerConfig {
  String name
  Int cpu_cores
  NetworkConfig network
  Array[String] services
}

enum Ports[Array[Int]] {
  DefaultPorts = [80, 443, 8080],
  CustomPorts = [3000, 3001]
}

enum Range[Pair[Int, Int]] {
  Small = (1, 100),
  Large = (1, 1000)
}

enum Settings[Map[String, Int]] {
  Timeouts = {"connect": 30, "read": 60, "write": 45},
  Retries = {"max": 3, "initial": 1}
}

enum Config[Object] {
  Development = object {port: 8080, debug: true, workers: 2},
  Production = object {port: 443, debug: false, workers: 8},
  Advanced = object {
    ports: [8080, 8081, 8082],
    settings: {"timeout": 30, "retries": 3},
    range: (1, 100),
    nested: object {enabled: true, level: 5}
  }
}

enum Servers[ServerConfig] {
  WebServer = ServerConfig {
    name: "web-01",
    cpu_cores: 4,
    network: NetworkConfig {
      hostname: "web.example.com",
      port: 443,
      tls: true
    },
    services: ["nginx", "php-fpm"]
  },
  Database = ServerConfig {
    name: "db-01",
    cpu_cores: 8,
    network: NetworkConfig {
      hostname: "db.example.com",
      port: 5432,
      tls: true
    },
    services: ["postgresql"]
  }
}

workflow enum_compound_values {
  output {
    Array[Int] default_ports = value(Ports.DefaultPorts)
    Array[Int] custom_ports = value(Ports.CustomPorts)
    Pair[Int, Int] small_range = value(Range.Small)
    Pair[Int, Int] large_range = value(Range.Large)
    Map[String, Int] timeouts = value(Settings.Timeouts)
    Map[String, Int] retries = value(Settings.Retries)
    Object dev_config = value(Config.Development)
    Object prod_config = value(Config.Production)
    Object advanced_config = value(Config.Advanced)
    ServerConfig web = value(Servers.WebServer)
    ServerConfig db = value(Servers.Database)
  }
}

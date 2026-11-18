update runs set backend = json_extract(backend, '$.type') where backend is not null;

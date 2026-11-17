alter table runs drop column backend_config;
alter table runs drop column backend_name;
alter table runs rename column backend_type to backend;

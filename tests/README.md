# Testing

This directory contains integration tests for *CDL* components.

Making sure they pass is vital to longevity of our application.

## Prerequisites

### Python
Tests run on python `3`. They are tested on CI using python `3.8`.

### Pip
We require `pip` to be present in `$PATH`. Some python installations, especially when you are keeping both `python2` and `python3`
on your machine, tend to reserve `pip` as `python2` package manager. Before using, please ensure whetreh that's the case on your system.
If it is, please adjust executable name to `pip3` in following commands, or symlink `pip3` to `pip`

### Rust & Cargo
Tests don't compile *CDL*. You have to do it yourself, and either install *CDL* binaries into your `$PATH` or point tests to binaries via env variables.

Supported env variables are:

| application | env |
|---|---|
| db-shrinker-postgres | DB_SHRINKER_POSTGRES_EXE |
| command-service | COMMAND_SERVICE_EXE |
| query-router | QUERY_ROUTER_EXE |
| schema-registry | SCHEMA_REGISTRY_EXE |
| query-service | QUERY_SERVICE_EXE |
| query-service-ts | QUERY_SERVICE_TS_EXE |

### Docker and docker-compose
We test using [testcontainers python library](https://pypi.org/project/testcontainers/), which spins up new docker environment for every test.
For this reason, docker and docker-compose must be present on the machine in order to run these tests.

## Running

1. First we need to initialize environment from `requirements.txt` located at the top of project directory.
We do that by running:
```
pip install -r ../requirements.txt
```

2. Then we need to compile protos:
```
python3 -m grpc_tools.protoc --proto_path="../" --python_out=. --grpc_python_out=. $(find ../rpc/proto/ -iname "*.proto")
```

3. Tests are discovered automatically, run them with:
```
pytest
```
## Structure

### ./data
Folder with test data, divided into directories per application.

### ./rpc
Directory generated by `grpc_tools.protoc` which contains python modules compiled from `rpc/proto/*.proto` files.

### application folders
eg. db_shrinker_postgres. Named after application they test.
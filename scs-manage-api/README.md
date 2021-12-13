## scs-manage-api

This is an internal API service intended to be triggered automatically or semi-automatically.
The service provides a number of API methods for managing the other SCS services:

- Starting/stopping/reloading the services
- Re-deploying the services
- Updating the configuration files

## Configuration

The service must be configured with a `ci-api` json file containing the locations of the `docker` and `config` directories as well as at least one API access key (300-bits of entropy minimum). The client must provide this key when calling endpoints with `Bearer` authentication.

## API Schema

| Endpoint                     | Method | Auth   | Response Type    | Description                                                                                                                                         |
| -----------                  | ------ | ------ | ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| /v1/configs                  | GET    | Bearer | JSON             | Returns the list of all editable configs with their current values.                                                                                 |
| /v1/up                       | POST   | Bearer | Streaming (JSON) | Forcefully starts the services by executing `docker-compose up -d`. Streams the execution logs to the client.                                       |
| /v1/down                     | POST   | Bearer | Streaming (JSON) | Forcefully stops the services by executing `docker-compose down`. Streams the execution logs to the client.                                         |
| /v1/restart                  | POST   | Bearer | Streaming (JSON) | Stops and restarts the services by combining the `down` and `up` commands, streaming the logs to the client. Terminates as soon as an error occurs. |
| /v1/deploy                   | POST   | Bearer | Streaming (JSON) | Pulls the latest changes, rebuilds the binaries, and restars the services, streaming the logs to the client. Terminates as soon as an error occurs. |
| /v1/is_running               | GET    | Bearer | JSON             | Returns "true" if there's a command running, "false" otherwise                                                                                      |
| /v1/last_command             | GET    | Bearer | JSON             | Returns the information about the last executed command, including its output                                                                       |
| /v1/services                 | GET    | Bearer | JSON             | Returns the list of the services with a boolean is_running status for each                                                                          |
| /v1/service/{name}/{command} | POST   | Bearer | JSON             | Applies the given {command} to the service {name}. The command must be one of (stop, start), the service name must be obtained from /services       |

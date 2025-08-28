# Prism LB

A simple layer 7 load balancer written in Rust providing the following functionality.

- Distributes client requests efficiently across multiple servers.
- Ensures high availability and reliability by sending requests only to servers that are online.

## Running the example

The following serves as an example of running and configuring `prism-lb`.

1. Build the container locally in the root directory (This will soon be added to docker hub).

```bash
docker build -t prism-lb:latest
```

2. Move to the example directory and execute the following.

```
docker compose up
```

3. The example compose file runs 3 instances of nginx and configures the lb based on the `example-backends.yaml` file. You can then curl
the endpoint to see the lb in action and see the connections being distributed in the compose logs.

```
$ curl 127.0.0.1:5001
...
example-target-1  | 172.23.0.3 - - [28/Aug/2025:07:48:34 +0000] "GET / HTTP/1.1" 200 615 "-" "curl/8.7.1" "-"
example-target-2  | 172.23.0.3 - - [28/Aug/2025:07:48:38 +0000] "GET / HTTP/1.1" 200 615 "-" "curl/8.7.1" "-"
example-target-3  | 172.23.0.3 - - [28/Aug/2025:07:48:39 +0000] "GET / HTTP/1.1" 200 615 "-" "curl/8.7.1" "-"
example-target-1  | 172.23.0.3 - - [28/Aug/2025:07:48:39 +0000] "GET / HTTP/1.1" 200 615 "-" "curl/8.7.1" "-"
example-target-2  | 172.23.0.3 - - [28/Aug/2025:07:48:40 +0000] "GET / HTTP/1.1" 200 615 "-" "curl/8.7.1" "-"
example-target-3  | 172.23.0.3 - - [28/Aug/2025:07:48:41 +0000] "GET / HTTP/1.1" 200 615 "-" "curl/8.7.1" "-"
```

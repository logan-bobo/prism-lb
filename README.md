# Prism LB

A simple layer 7 load balancer written in Rust providing the following functionality.

- Distributes client requests efficiently across multiple servers.
- Ensures high availability and reliability by sending requests only to servers that are online.

## Running the example

The following serves as an example of running and configuring `prism-lb`.

1. Build the container locally in the root directory (This will soon be added to docker hub).

```bash
$ docker build -t prism-lb:latest
```

2. Move to the example directory and execute the following.

```bash
$ docker compose up
```

3. The example compose file runs 3 instances of nginx and configures the lb based on the `example-backends.yaml` file. You can then curl
the endpoint to see the lb in action and see the connections being distributed in the compose logs.

```bash
$ curl 127.0.0.1:5001
...
example-target-1  | 172.23.0.3 - - [28/Aug/2025:07:48:34 +0000] "GET / HTTP/1.1" 200 615 "-" "curl/8.7.1" "-"
example-target-2  | 172.23.0.3 - - [28/Aug/2025:07:48:38 +0000] "GET / HTTP/1.1" 200 615 "-" "curl/8.7.1" "-"
example-target-3  | 172.23.0.3 - - [28/Aug/2025:07:48:39 +0000] "GET / HTTP/1.1" 200 615 "-" "curl/8.7.1" "-"
example-target-1  | 172.23.0.3 - - [28/Aug/2025:07:48:39 +0000] "GET / HTTP/1.1" 200 615 "-" "curl/8.7.1" "-"
example-target-2  | 172.23.0.3 - - [28/Aug/2025:07:48:40 +0000] "GET / HTTP/1.1" 200 615 "-" "curl/8.7.1" "-"
example-target-3  | 172.23.0.3 - - [28/Aug/2025:07:48:41 +0000] "GET / HTTP/1.1" 200 615 "-" "curl/8.7.1" "-"
```
## Performance

The performance is measured using the compose example on my M2 Macbook Pro. With two cases.

1. 500 connections for 10 seconds.

```
bombardier -c 500 http://127.0.0.1:5001 --latencies --fasthttp
Bombarding http://127.0.0.1:5001 for 10s using 500 connection(s)
Statistics        Avg      Stdev        Max
  Reqs/sec     34182.53    5716.47   49938.00
  Latency       14.61ms     4.48ms   140.08ms
  Latency Distribution
     50%    13.45ms
     75%    16.94ms
     90%    21.09ms
     95%    24.09ms
     99%    32.75ms
  HTTP codes:
    1xx - 0, 2xx - 342293, 3xx - 0, 4xx - 0, 5xx - 0
    others - 0
  Throughput:    29.73MB/s
  ```

2. 250 connections for 10 seconds.

```
bombardier -c 250 http://127.0.0.1:5001 --latencies --fasthttp
Bombarding http://127.0.0.1:5001 for 10s using 250 connection(s)
Statistics        Avg      Stdev        Max
  Reqs/sec     29543.63    4738.51   42027.17
  Latency        8.45ms     3.70ms    95.42ms
  Latency Distribution
     50%     7.72ms
     75%     9.69ms
     90%    11.85ms
     95%    13.38ms
     99%    19.37ms
  HTTP codes:
    1xx - 0, 2xx - 295966, 3xx - 0, 4xx - 0, 5xx - 0
    others - 0
  Throughput:    25.71MB/s
```

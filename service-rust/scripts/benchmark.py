#!/usr/bin/env python3
"""Measure one running Sun-Panel service against its existing navigation data.

Uses only Python's standard library. It performs read-only navigation requests
against the single-user service without authentication; it does not seed data,
open a database, or compare the result with another backend.
"""

import argparse
import collections
import concurrent.futures
import datetime
import http.client
import json
import math
import os
import platform
import socket
import statistics
import subprocess
import sys
import time
import urllib.parse


MAX_REQUESTS = 100_000
MAX_CONCURRENCY = 32
MAX_RESPONSE_BYTES = 16 * 1024 * 1024
WARMUP_REQUESTS = 5


class BenchmarkError(Exception):
    """An intentionally sanitized error that cannot contain response secrets."""


class Client:
    def __init__(self, target, timeout):
        self.target = target
        connection_type = (
            http.client.HTTPSConnection
            if target.scheme == "https"
            else http.client.HTTPConnection
        )
        self.connection = connection_type(target.hostname, target.port, timeout=timeout)
        self.headers = {
            "Content-Type": "application/json",
            "Accept": "application/json",
            "Connection": "keep-alive",
        }
    def close(self):
        self.connection.close()

    def post(self, path, data=None):
        body = json.dumps(data if data is not None else {}, ensure_ascii=False).encode("utf-8")
        try:
            self.connection.request(
                "POST", self.target.path.rstrip("/") + path, body, self.headers
            )
            response = self.connection.getresponse()
            payload = response.read(MAX_RESPONSE_BYTES + 1)
            status = response.status
            if len(payload) > MAX_RESPONSE_BYTES:
                raise BenchmarkError("response_too_large")
            if not 200 <= status < 300:
                raise BenchmarkError("http_{}".format(status))
            try:
                value = json.loads(payload)
            except (ValueError, UnicodeError):
                raise BenchmarkError("invalid_json") from None
            if not isinstance(value, dict):
                raise BenchmarkError("invalid_response_shape")
            code = value.get("code")
            if type(code) is not int:
                raise BenchmarkError("missing_business_code")
            if code != 0:
                # Never include the server's message or raw response body.
                raise BenchmarkError("business_code_{}".format(code))
            return value
        except BenchmarkError:
            self.close()
            raise
        except (socket.timeout, TimeoutError):
            self.close()
            raise BenchmarkError("timeout") from None
        except (OSError, http.client.HTTPException) as error:
            self.close()
            raise BenchmarkError("connection_" + type(error).__name__) from None


def dataset_size(value):
    data = value.get("data")
    groups = data.get("list") if isinstance(data, dict) else None
    if not isinstance(groups, list):
        raise BenchmarkError("invalid_navigation_shape")
    items = 0
    for group in groups:
        if not isinstance(group, dict) or not isinstance(group.get("items"), list):
            raise BenchmarkError("invalid_navigation_shape")
        items += len(group["items"])
    return len(groups), items


def percentile(sorted_values, percent):
    """Linear interpolation, including the one-sample case."""
    if not sorted_values:
        return None
    position = (len(sorted_values) - 1) * percent / 100.0
    lower = math.floor(position)
    upper = math.ceil(position)
    return sorted_values[lower] + (
        sorted_values[upper] - sorted_values[lower]
    ) * (position - lower)


def worker(target, timeout, request_count):
    # Each worker owns exactly one HTTPConnection and reuses it throughout its
    # loop. A broken connection is closed and the next attempt reconnects.
    client = Client(target, timeout)
    latencies = []
    errors = collections.Counter()
    datasets = set()
    started = time.perf_counter()
    try:
        for _ in range(request_count):
            before = time.perf_counter()
            try:
                value = client.post("/api/panel/navigation")
                datasets.add(dataset_size(value))
                latencies.append((time.perf_counter() - before) * 1000.0)
            except BenchmarkError as error:
                errors[str(error)] += 1
    finally:
        finished = time.perf_counter()
        client.close()
    return started, finished, latencies, errors, datasets


def ps_snapshot(pid):
    result = {
        "pid": pid,
        "sampled_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "semantics": (
            "A ps sample, not peak usage. RSS is KiB; CPU is the process CPU "
            "percentage reported by ps and may average over process lifetime."
        ),
    }
    environment = dict(os.environ, LC_ALL="C")
    try:
        sample = subprocess.run(
            ["ps", "-p", str(pid), "-o", "pid=,rss=,%cpu="],
            capture_output=True, text=True, timeout=3, env=environment,
        )
        fields = sample.stdout.split()
        if sample.returncode != 0 or len(fields) != 3:
            result.update(available=False, reason="process_not_found_or_ps_failed")
            return result
        result.update(available=True, rss_kib=int(fields[1]), cpu_percent=float(fields[2]))
        if sys.platform == "darwin":
            # macOS -M adds a thread table even with a custom -o format. Its
            # first row starts with USER/PID and later rows start with PID.
            command = ["ps", "-M", "-p", str(pid)]
        else:
            command = ["ps", "-p", str(pid), "-o", "nlwp="]
        threads = subprocess.run(
            command, capture_output=True, text=True, timeout=3, env=environment,
        )
        if threads.returncode == 0:
            if sys.platform == "darwin":
                counts = [
                    line for line in threads.stdout.splitlines()
                    if line.split() and (
                        line.split()[0] == str(pid)
                        or (len(line.split()) > 1 and line.split()[1] == str(pid))
                    )
                ]
                result["threads"] = len(counts) or None
            else:
                result["threads"] = int(threads.stdout.strip())
        else:
            result["threads"] = None
    except (OSError, ValueError, subprocess.TimeoutExpired):
        result.update(available=False, reason="ps_unavailable_or_sample_failed")
    return result


def arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:3002")
    parser.add_argument("--requests", type=int, default=500)
    parser.add_argument("--concurrency", type=int, default=4)
    parser.add_argument("--timeout", type=float, default=10.0, help="Per-request timeout in seconds")
    parser.add_argument("--pid", type=int, help="Optional service PID for before/after ps samples")
    args = parser.parse_args()
    if not 1 <= args.requests <= MAX_REQUESTS:
        parser.error("--requests must be between 1 and {}".format(MAX_REQUESTS))
    if not 1 <= args.concurrency <= MAX_CONCURRENCY:
        parser.error("--concurrency must be between 1 and {}".format(MAX_CONCURRENCY))
    if not math.isfinite(args.timeout) or not 0.1 <= args.timeout <= 120:
        parser.error("--timeout must be between 0.1 and 120 seconds")
    if args.pid is not None and not 1 <= args.pid <= 2**31 - 1:
        parser.error("--pid must be a positive process ID")
    try:
        target = urllib.parse.urlsplit(args.url)
        if (
            target.scheme not in ("http", "https") or not target.hostname
            or target.username is not None or target.password is not None
            or target.query or target.fragment
        ):
            raise ValueError()
        _ = target.port
    except ValueError:
        parser.error("--url must be an HTTP(S) URL without credentials, query, or fragment")
    return args, target


def main():
    args, target = arguments()
    warmup = Client(target, args.timeout)
    try:
        for _ in range(WARMUP_REQUESTS):
            dataset = dataset_size(warmup.post("/api/panel/navigation"))
    except BenchmarkError as error:
        print("benchmark: navigation warmup failed ({})".format(error), file=sys.stderr)
        return 1
    finally:
        warmup.close()

    concurrency = min(args.requests, args.concurrency)
    before = ps_snapshot(args.pid) if args.pid is not None else None
    with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as pool:
        futures = [
            pool.submit(worker, target, args.timeout,
                        args.requests // concurrency + (index < args.requests % concurrency))
            for index in range(concurrency)
        ]
        results = [future.result() for future in futures]
    after = ps_snapshot(args.pid) if args.pid is not None else None
    elapsed = max(result[1] for result in results) - min(result[0] for result in results)
    samples = sorted(value for result in results for value in result[2])
    errors = sum((result[3] for result in results), collections.Counter())
    observed = set().union(*(result[4] for result in results))
    report = {
        "scope": (
            "Only this running single-user service and existing dataset, without "
            "authentication; this is not a Go comparison."
        ),
        "environment": {
            "url": urllib.parse.urlunsplit(target).rstrip("/"),
            "python": platform.python_version(), "platform": platform.platform(),
            "machine": platform.machine(), "cpu_count": os.cpu_count(),
            "requests": args.requests, "concurrency": concurrency,
            "requested_concurrency": args.concurrency, "timeout_seconds": args.timeout,
        },
        "method": {
            "endpoint": "POST /api/panel/navigation",
            "access": "Single-user workspace, without authentication.",
            "warmup_requests": WARMUP_REQUESTS,
            "connections": "One persistent HTTPConnection per worker; no retries of failed attempts.",
            "latency": "Successful HTTP round trips, including body read and JSON validation; milliseconds.",
            "timing": "Worker loops only; includes each worker's first connection establishment.",
            "data": "Existing navigation data only; no seeding or database file access.",
        },
        "dataset": {
            "groups": dataset[0], "items": dataset[1],
            "changed_during_measurement": any(size != dataset for size in observed),
        },
        "measurement": {
            "elapsed_seconds": elapsed, "successful_requests": len(samples),
            "errors": sum(errors.values()), "error_types": dict(errors),
            "throughput_requests_per_second": len(samples) / elapsed if elapsed else None,
            "attempts_per_second": args.requests / elapsed if elapsed else None,
            "latency_ms": {
                "p50": percentile(samples, 50), "p95": percentile(samples, 95),
                "p99": percentile(samples, 99),
                "mean": statistics.mean(samples) if samples else None,
                "min": samples[0] if samples else None, "max": samples[-1] if samples else None,
            },
        },
    }
    if args.pid is not None:
        report["process_samples"] = {"before": before, "after": after}
    print(json.dumps(report, ensure_ascii=False, indent=2, allow_nan=False))
    return 1 if errors else 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        print("benchmark: interrupted", file=sys.stderr)
        sys.exit(130)

#!/usr/bin/env python3
"""Test-only SOCKS5 hop for chain and route checks on a real machine.

Listens on 127.0.0.1, accepts unauthenticated CONNECT requests, forwards them
to the requested target, and appends one line per request to the log file:

    python socks-hop.py <name> <port> <log-file>

Use two instances (for example A on 1081 and C on 1083) and a chain whose
second hop dials through the first. The log shows which hop each request
reached. Never point it at a real network or accept it as a production proxy.
"""
import socket
import struct
import sys
import threading

name, port, log = sys.argv[1], int(sys.argv[2]), sys.argv[3]


def note(message):
    with open(log, "a", encoding="utf-8") as file:
        file.write(f"{name} {message}\n")


def relay(source, sink):
    try:
        while True:
            data = source.recv(65536)
            if not data:
                break
            sink.sendall(data)
    except OSError:
        pass
    finally:
        for side in (source, sink):
            try:
                side.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass


def handle(client):
    try:
        client.recv(256)
        client.sendall(b"\x05\x00")
        request = client.recv(4)
        if request[3] == 1:
            host = socket.inet_ntoa(client.recv(4))
        elif request[3] == 3:
            host = client.recv(client.recv(1)[0]).decode()
        else:
            return
        target_port = struct.unpack("!H", client.recv(2))[0]
        note(f"CONNECT {host}:{target_port}")
        upstream = socket.create_connection((host, target_port))
        client.sendall(b"\x05\x00\x00\x01" + b"\x00" * 6)
        threading.Thread(target=relay, args=(client, upstream), daemon=True).start()
        relay(upstream, client)
    except Exception as error:  # the log records why a request failed
        note(f"error {error}")
    finally:
        client.close()


server = socket.socket()
server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
server.bind(("127.0.0.1", port))
server.listen(16)
while True:
    connection, _ = server.accept()
    threading.Thread(target=handle, args=(connection,), daemon=True).start()

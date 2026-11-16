# T-358 step 1: the stand-in HTTP CONNECT proxy of the row proxy-only.
#
# Usage: python3 -I connect-proxy.py PORT LOG NAME:PORT=TCPPORT [MODE]
#
# The proxy listens on 127.0.0.1:PORT. It reads the request head of each
# connection and appends the request line to LOG ("request CONNECT
# NAME:PORT HTTP/1.1"). It connects only the target NAME:PORT, and only to
# 127.0.0.1:TCPPORT: it answers 200 and copies the bytes both ways. Each
# other target gets 403, and each other method gets 405. MODE refuse (a
# planted defect) answers 403 to each request. When the socket listens, the
# proxy prints "ready" on stdout.
import socket
import socketserver
import sys
import threading


def pipe(src, dst):
    try:
        while True:
            chunk = src.recv(65536)
            if not chunk:
                break
            dst.sendall(chunk)
    except OSError:
        pass
    finally:
        try:
            dst.shutdown(socket.SHUT_WR)
        except OSError:
            pass


def read_head(conn):
    data = b""
    while b"\r\n\r\n" not in data:
        chunk = conn.recv(4096)
        if not chunk or len(data) > 65536:
            return None
        data += chunk
    return data


class Connect(socketserver.BaseRequestHandler):
    def handle(self):
        srv = self.server
        conn = self.request
        try:
            head = read_head(conn)
        except OSError:
            return
        if head is None:
            return
        line = head.split(b"\r\n", 1)[0].decode("ascii", "replace")
        with srv.lock:
            with open(srv.log, "a") as f:
                f.write("request %s\n" % line)
        words = line.split()
        if len(words) != 3 or words[0] != "CONNECT":
            conn.sendall(b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\n\r\n")
            return
        if srv.refuse or words[1] != srv.name:
            conn.sendall(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")
            return
        try:
            up = socket.create_connection(("127.0.0.1", srv.tcp_port), timeout=10)
        except OSError:
            conn.sendall(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
            return
        up.settimeout(None)
        conn.sendall(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        t = threading.Thread(target=pipe, args=(up, conn), daemon=True)
        t.start()
        pipe(conn, up)
        t.join()
        up.close()


class Server(socketserver.ThreadingMixIn, socketserver.TCPServer):
    daemon_threads = True
    allow_reuse_address = True


def main():
    if len(sys.argv) not in (4, 5) or "=" not in sys.argv[3]:
        print("usage: connect-proxy.py PORT LOG NAME:PORT=TCPPORT [refuse]", file=sys.stderr)
        return 64
    srv = Server(("127.0.0.1", int(sys.argv[1])), Connect)
    srv.log = sys.argv[2]
    srv.name, tcp_port = sys.argv[3].split("=")
    srv.tcp_port = int(tcp_port)
    srv.refuse = len(sys.argv) == 5 and sys.argv[4] == "refuse"
    srv.lock = threading.Lock()
    print("ready", flush=True)
    srv.serve_forever()
    return 0


sys.exit(main())

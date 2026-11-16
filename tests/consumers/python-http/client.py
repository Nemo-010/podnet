# T-358 step 3: the HTTP client of a consumer under the vnet shim.
#
# Usage: python3 -I client.py LIBRARY URL [LIBDIR]
#
# LIBRARY is urllib (the standard library) or requests. LIBDIR is a
# directory with the package requests and its dependencies: the script adds
# it to sys.path. The client sends GET to URL. It honours HTTP_PROXY, as
# each library does by default. It prints the status and the body. It
# exits 0 on HTTP 200, else 1.
import sys


def main():
    if len(sys.argv) not in (3, 4) or sys.argv[1] not in ("urllib", "requests"):
        print("usage: client.py urllib|requests URL [LIBDIR]", file=sys.stderr)
        return 64
    library, url = sys.argv[1], sys.argv[2]
    if len(sys.argv) == 4:
        sys.path.insert(0, sys.argv[3])
    try:
        if library == "urllib":
            import urllib.request

            with urllib.request.urlopen(url, timeout=20) as response:
                status, body = response.status, response.read()
        else:
            import requests

            response = requests.get(url, timeout=20)
            status, body = response.status_code, response.content
    except Exception as e:  # each failure is an answer: print it
        print("error: %s: %s" % (type(e).__name__, e), file=sys.stderr)
        cause = e.__cause__ or e.__context__
        while cause is not None:
            print("inner: %s: %s" % (type(cause).__name__, cause), file=sys.stderr)
            cause = cause.__cause__ or cause.__context__
        return 1
    print("status %d" % status)
    print(body.decode("utf-8", "replace"))
    return 0 if status == 200 else 1


sys.exit(main())

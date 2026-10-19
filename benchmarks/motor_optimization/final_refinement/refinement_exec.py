"""Redirect a single native request, then replace this tracked process with it."""
import hashlib
import os
from pathlib import Path
import sys


def main():
    stdin, stdout, stderr, binary, digest, *args = sys.argv[1:]
    if hashlib.sha256(Path(binary).read_bytes()).hexdigest() != digest:
        raise ValueError('native executable changed before exec')
    with (open(stdin, 'rb') as source, open(stdout, 'xb') as output,
          open(stderr, 'xb') as errors):
        os.dup2(source.fileno(), 0)
        os.dup2(output.fileno(), 1)
        os.dup2(errors.fileno(), 2)
    os.execve(binary, [binary, *args], os.environ)


if __name__ == '__main__':
    main()

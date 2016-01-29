import json
import sys
import socket
import os
import os.path
import time

mozSearchPath = sys.argv[1]
indexPath = sys.argv[2]

class CodeSearch:
    def __init__(self, host, port):
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.sock.connect((host, port))
        self.state = 'init'
        self.buffer = ''
        self.matches = []
        self.wait_ready()
        self.query = None

    def close(self):
        self.sock.close()

    def collateMatches(self, matches):
        paths = {}
        for m in matches:
            paths.setdefault(m['path'], []).append({
                'lno': m['lno'],
                'bounds': m['bounds'],
                'line': m['line']
            })
        results = [ {'path': p, 'icon': '', 'lines': paths[p]} for p in paths ]
        return results

    def search(self, pattern, fold_case=True, file='.*', repo='.*'):
        query = {'body': {'fold_case': fold_case, 'line': pattern, 'file': file, 'repo': repo}}
        self.query = json.dumps(query)
        self.state = 'search'
        self.sock.sendall(self.query + '\n')
        self.wait_ready()
        matches = self.collateMatches(self.matches)
        self.matches = []
        return matches

    def wait_ready(self):
        while self.state != 'ready':
            input = self.sock.recv(1024)
            self.buffer += input
            self.handle_input()

    def handle_input(self):
        try:
            pos = self.buffer.index('\n')
        except:
            pos = -1

        if pos >= 0:
            line = self.buffer[:pos]
            self.buffer = self.buffer[pos + 1:]
            self.handle_line(line)
            self.handle_input()

    def handle_line(self, line):
        j = json.loads(line)
        if j['opcode'] == 'match':
            self.matches.append(j['body'])
        elif j['opcode'] == 'ready':
            self.state = 'ready'
        elif j['opcode'] == 'done':
            if j.get('body', {}).get('why') == 'timeout':
                print 'Timeout', self.query
        elif j['opcode'] == 'error':
            self.matches = []
        else:
            print 'Unknown opcode %s' % j['opcode']
            raise BaseException()

def daemonize(args):
    # Spawn a process to start the daemon
    pid = os.fork()
    if pid:
        # Parent
        return

    # Double fork
    pid = os.fork()
    if pid:
        os._exit(0)

    pid = os.fork()
    if pid:
        os._exit(0)

    print 'Running codesearch (pid %d)' % os.getpid()
    os.execv(args[0], args)

def startup_codesearch():
    path = os.environ['CODESEARCH']
    if not path:
        return

    args = [path, '-listen', 'tcp://localhost:8080',
            '-load_index', os.path.join(indexPath, 'livegrep.idx'),
            '-max_matches', '1000', '-timeout', '10000']

    daemonize(args)
    time.sleep(5)

def search(pattern, fold_case=True, file='.*', repo='.*'):
    try:
        codesearch = CodeSearch('localhost', 8080)
    except socket.error, e:
        startup_codesearch()
        try:
            codesearch = CodeSearch('localhost', 8080)
        except socket.error, e:
            return []

    try:
        return codesearch.search(pattern, fold_case, file, repo)
    finally:
        codesearch.close()

"""Actual terminal progress against a tiny local Go distribution fixture."""
import errno, fcntl, hashlib, http.server, io, json, os, pty, select, shutil, struct, subprocess, tarfile, termios, time
from pathlib import Path
from support import BIN, REPORTS
from https_fixture import create_https_fixture

def verify_progress(fixture):
    fixture=fixture/'progress';fixture.mkdir()
    archive=io.BytesIO()
    with tarfile.open(fileobj=archive,mode='w:gz') as output:
        for name,payload in [('go/bin/go',b'#!/bin/sh\necho fixture-go\n'),('go/bin/gofmt',b'#!/bin/sh\nexit 0\n'),('go/padding',os.urandom(1024*1024))]:
            entry=tarfile.TarInfo(name);entry.size=len(payload);entry.mode=0o755;output.addfile(entry,io.BytesIO(payload))
    body=archive.getvalue();digest=hashlib.sha256(body).hexdigest()
    filename='go1.27.0.linux-amd64.tar.gz'
    metadata=json.dumps([{'version':'go1.27.0','stable':True,'files':[{'filename':filename,'os':'linux','arch':'amd64','version':'go1.27.0','sha256':digest,'size':len(body),'kind':'archive'}]}]).encode()
    state={'known':True,'corrupt':False,'downloads':0}
    class Origin(http.server.BaseHTTPRequestHandler):
        protocol_version='HTTP/1.1'
        def log_message(self,*_): pass
        def do_GET(self):
            if self.path.startswith('/dl/?'):
                payload=metadata;download=False
            elif self.path=='/dl/'+filename:
                payload=b'bad archive' if state['corrupt'] else body;download=True;state['downloads']+=1
            else:
                self.send_error(404);return
            self.send_response(200)
            chunked=download and not state['known']
            if chunked: self.send_header('Transfer-Encoding','chunked')
            else: self.send_header('Content-Length',str(len(payload)))
            self.send_header('Connection','close');self.end_headers()
            for offset in range(0,len(payload),32768):
                block=payload[offset:offset+32768]
                if chunked: self.wfile.write(f'{len(block):x}\r\n'.encode())
                self.wfile.write(block)
                if chunked: self.wfile.write(b'\r\n')
                self.wfile.flush()
                if download: time.sleep(0.01)
            if chunked: self.wfile.write(b'0\r\n\r\n');self.wfile.flush()
    server,proxy_env=create_https_fixture(fixture,['go.dev'],Origin)
    env={**os.environ,**proxy_env,'HOME':str(fixture),'PINSET_HOME':str(fixture/'home'),'TERM':'xterm','LANG':'en_US.UTF-8'}
    for key in ['PINSET_IDENTITY','PINSET_PROFILE','PINSET_NO_ENV','PINSET_ENV_RESOLVED']:env.pop(key,None)
    root=fixture/'project';root.mkdir()
    def command(*args,expected=0,terminal=True):
        argv=[str(BIN),'-C',str(root),'--lang','en',*args]
        if not terminal:
            process=subprocess.run(argv,env=env,cwd=root,text=True,capture_output=True,timeout=60)
            assert process.returncode==expected,process.stdout+process.stderr
            return process.stdout,process.stderr
        master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',30,180,0,0))
        process=subprocess.Popen(argv,env=env,cwd=root,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=slave)
        os.close(slave);chunks=[];deadline=time.monotonic()+60
        try:
            while True:
                assert time.monotonic()<deadline,'terminal progress timeout'
                if select.select([master],[],[],0.1)[0]:
                    try: block=os.read(master,65536)
                    except OSError as error:
                        if error.errno==errno.EIO:break
                        raise
                    if not block:break
                    chunks.append(block)
            stdout=process.communicate(timeout=5)[0].decode()
            stderr=b''.join(chunks).decode()
            assert process.returncode==expected,stdout+stderr
            return stdout,stderr
        finally:
            os.close(master)
            if process.poll() is None:process.kill();process.wait()
    try:
        command('init',terminal=False)
        _,terminal=command('use','go@latest')
        for token in ['Resolving official version','Downloading',filename,'%','ETA','Extracting verified artifact','Installed']:
            assert token in terminal,(token,terminal)
        (REPORTS/'progress-terminal-known.log').write_text(terminal)
        downloads=state['downloads']
        _,reused=command('install')
        assert 'Already installed; reused' in reused and 'Downloading' not in reused
        assert state['downloads']==downloads
        shutil.rmtree(fixture/'home/v3/installs')
        _,cached=command('install','--offline')
        assert 'Verified cache' in cached and 'Downloading' not in cached
        assert state['downloads']==downloads
        stdout,stderr=command('--json','install')
        assert json.loads(stdout)['installed'] and stderr==''
        _,stderr=command('install',terminal=False)
        assert stderr==''
        # Use separate owned homes to exercise unknown length and failed integrity.
        env['PINSET_HOME']=str(fixture/'unknown-home');state['known']=False
        _,unknown=command('install')
        (REPORTS/'progress-terminal-unknown.log').write_text(unknown)
        assert 'Downloading' in unknown and '%' not in unknown and 'ETA' not in unknown,unknown
        env['PINSET_HOME']=str(fixture/'json-home');state['known']=True
        stdout,stderr=command('--json','install')
        assert json.loads(stdout)['installed'] and stderr==''
        env['PINSET_HOME']=str(fixture/'failure-home');state['corrupt']=True
        _,failed=command('install',expected=1)
        (REPORTS/'progress-terminal-failed.log').write_text(failed)
        assert 'artifact integrity mismatch' in failed.lower() and 'Installed' not in failed,failed
        assert not list((fixture/'failure-home/v3/installs').rglob('.pinset-install.toml'))
        return {'terminal':'known-size bar with bytes/speed/ETA; unknown-size spinner without percentage',
                'cache':'verified cache and existing install explicitly distinguished','json':'silent stderr even on TTY',
                'redirected':'no terminal control sequences','integrity_failure':'no completed installation'}
    finally:
        server.shutdown();server.server_close()

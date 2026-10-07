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
    state={'known':True,'corrupt':False,'downloads':0,'cuts':0,'cut_bytes':len(body)//4,'ranges':[],'identity':[],'status':200}
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
            if download:
                state['identity'].append(self.headers.get('Accept-Encoding'))
                if state['status']!=200:
                    self.send_response(state['status']);self.send_header('Content-Length','0');self.send_header('Connection','close');self.end_headers();return
                header=self.headers.get('Range')
                start=int(header[6:-1]) if header else 0
                if start:
                    state['ranges'].append(start)
                    self.send_response(206)
                    self.send_header('Content-Range',f'bytes {start}-{len(body)-1}/{len(body)}')
                    payload=payload[start:]
                else:self.send_response(200)
            else:self.send_response(200)
            chunked=download and not state['known']
            if chunked: self.send_header('Transfer-Encoding','chunked')
            else: self.send_header('Content-Length',str(len(payload)))
            self.send_header('Connection','close');self.end_headers()
            if download and state['cuts']:
                state['cuts']-=1
                self.wfile.write(payload[:state['cut_bytes']]);self.wfile.flush();return
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
        # Three body interruptions reproduce the reported Go failure. A fourth
        # response succeeds, and the terminal explains each retained prefix.
        env['PINSET_HOME']=str(fixture/'retry-home');state['cuts']=3;state['ranges']=[]
        _,retried=command('use','--global','go@latest')
        assert 'retrying 4/8' in retried and 'resume at' in retried,retried
        assert state['ranges']==[len(body)//4,len(body)//4*2,len(body)//4*3],state
        assert all(value=='identity' for value in state['identity']),state
        assert len(list((fixture/'retry-home/v3/installs').rglob('.pinset-install.toml')))==1
        (REPORTS/'progress-terminal-retried.log').write_text(retried)
        # Exhausted transfers retain a safe partial, commit no new selection,
        # and resume across invocations. JSON progress must remain silent.
        env['PINSET_HOME']=str(fixture/'exhausted-home');state['cuts']=8;state['cut_bytes']=len(body)//16;state['ranges']=[]
        stdout,stderr=command('--json','use','--global','go@latest',expected=1)
        failure=json.loads(stdout);assert 'after 8 attempts' in str(failure) and stderr=='',failure
        assert not (fixture/'exhausted-home/v3/global/config.toml').exists()
        assert not list((fixture/'exhausted-home/v3/installs').rglob('.pinset-install.toml'))
        partial=list((fixture/'exhausted-home/v3/cache').rglob('*.part'))
        assert len(partial)==1 and partial[0].stat().st_size>0,partial
        state['cuts']=0;state['ranges']=[]
        stdout,stderr=command('--json','use','--global','go@latest')
        assert json.loads(stdout) and stderr=='' and state['ranges'],(stdout,stderr,state)
        assert not partial[0].exists()
        before={path:path.read_bytes() for path in (fixture/'exhausted-home/v3/global').glob('*.toml')}
        shutil.rmtree(fixture/'exhausted-home/v3/installs');shutil.rmtree(fixture/'exhausted-home/v3/cache')
        state['status']=404;downloads=state['downloads']
        command('use','--global','go@latest',expected=1,terminal=False)
        assert state['downloads']==downloads+1
        assert all(path.read_bytes()==content for path,content in before.items())
        state['status']=200
        env['PINSET_HOME']=str(fixture/'failure-home');state['corrupt']=True
        _,failed=command('install',expected=1)
        (REPORTS/'progress-terminal-failed.log').write_text(failed)
        assert 'artifact integrity mismatch' in failed.lower() and 'Installed' not in failed,failed
        assert not list((fixture/'failure-home/v3/installs').rglob('.pinset-install.toml'))
        return {'terminal':'known-size bar with bytes/speed/ETA; unknown-size spinner without percentage',
                'cache':'verified cache and existing install explicitly distinguished','json':'silent stderr even on TTY',
                'redirected':'no terminal control sequences','integrity_failure':'no completed installation',
                'download_recovery':'three interruptions resume successfully; eight attempts bounded; next invocation resumes retained partial',
                'failed_global_use':'no new selection on failure; previous config/lock unchanged; HTTP 404 is not retried'}
    finally:
        server.shutdown();server.server_close()

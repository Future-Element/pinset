"""Local official-host HTTPS fixtures with an ephemeral CA and verified TLS."""
import http.server, ssl, threading
from support import run

def create_https_fixture(directory, hosts, origin):
    ca, cert, key = directory/'ca.pem', directory/'server.pem', directory/'server.key'
    run(['openssl','req','-x509','-newkey','rsa:2048','-nodes','-days','1','-subj','/CN=Pinset local verification CA',
         '-keyout',directory/'ca.key','-out',ca])
    run(['openssl','req','-newkey','rsa:2048','-nodes','-subj','/CN='+hosts[0],
         '-keyout',key,'-out',directory/'server.csr'])
    extensions=directory/'server.extensions'
    extensions.write_text('basicConstraints=CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName='+','.join('DNS:'+host for host in hosts)+'\n')
    run(['openssl','x509','-req','-in',directory/'server.csr','-CA',ca,'-CAkey',directory/'ca.key',
         '-CAcreateserial','-out',cert,'-days','1','-extfile',extensions])
    tls=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);tls.load_cert_chain(cert,key)
    class Proxy(http.server.BaseHTTPRequestHandler):
        def log_message(self,*_): pass
        def do_CONNECT(self):
            if self.path not in [host+':443' for host in hosts]:
                self.send_error(403);return
            self.send_response(200,'Connection established');self.end_headers()
            with tls.wrap_socket(self.connection,server_side=True) as connection:
                origin(connection,self.client_address,self.server)
            self.close_connection=True
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Proxy)
    threading.Thread(target=server.serve_forever,daemon=True).start()
    proxy='http://127.0.0.1:'+str(server.server_port)
    return server,{'PINSET_CA_BUNDLE':str(ca),'HTTPS_PROXY':proxy,'https_proxy':proxy,
                   'HTTP_PROXY':proxy,'http_proxy':proxy,'NO_PROXY':'','no_proxy':'','ALL_PROXY':'','all_proxy':''}

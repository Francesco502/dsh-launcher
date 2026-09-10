// Child-process registry: execFileSync npm must not block the HTTP server.
const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const root = process.argv[2];
const server = http.createServer((req,res) => {
  const name = decodeURIComponent(req.url.split('?')[0].slice(1));
  if (name.endsWith('.tgz')) {
    const file=path.join(root,path.basename(name));
    if (!fs.existsSync(file)) {res.writeHead(404);res.end();return;}
    res.end(fs.readFileSync(file));return;
  }
  if (!['qa-selected','qa-unselected'].includes(name)) {res.writeHead(404);res.end('{}');return;}
  const versions={};
  for (const version of ['1.0.0','2.0.0-beta.2','2.0.0-beta.10']) {
    const file=`${name}-${version}.tgz`;
    const bytes=fs.readFileSync(path.join(root,file));
    versions[version]={name,version,dist:{tarball:`http://127.0.0.1:${server.address().port}/${file}`,integrity:'sha512-'+crypto.createHash('sha512').update(bytes).digest('base64')}};
  }
  res.setHeader('Content-Type','application/json');res.end(JSON.stringify({name,'dist-tags':{latest:'1.0.0'},versions}));
});
server.listen(0,'127.0.0.1',()=>process.send(server.address().port));

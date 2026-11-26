// node --test supabase/functions/alpha-admin/mail.test.mjs  (Node 24 strips the TypeScript types)
import test from 'node:test';
import assert from 'node:assert/strict';
import net from 'node:net';
import { SmtpClient, SmtpError, buildMessage, encodeWords, fill, ADDRESS } from './mail.ts';

function nodeConn(socket) {
  const queue = []; let waiter = null; let ended = false;
  socket.on('data', d => { if (waiter) { const w = waiter; waiter = null; w(d); } else queue.push(d); });
  socket.on('end', () => { ended = true; if (waiter) { const w = waiter; waiter = null; w(null); } });
  return {
    async read(p) {
      const d = queue.length ? queue.shift() : ended ? null : await new Promise(r => { waiter = r; });
      if (!d) return null;
      const n = Math.min(p.length, d.length); p.set(d.subarray(0, n)); if (n < d.length) queue.unshift(d.subarray(n));
      return n;
    },
    async write(p) { socket.write(Buffer.from(p)); return p.length; },
    close() { socket.end(); }
  };
}

// Fake server: records commands and the DATA payload; rejects AUTH unless the password matches.
function fakeServer(password) {
  const seen = { lines: [], messages: [] };
  const server = net.createServer(sock => {
    let buf = ''; let inData = false; let data = [];
    sock.write('220 fake ESMTP\r\n');
    sock.on('data', chunk => {
      buf += chunk.toString('utf8');
      let i;
      while ((i = buf.indexOf('\r\n')) >= 0) {
        const line = buf.slice(0, i); buf = buf.slice(i + 2);
        if (inData) {
          if (line === '.') { inData = false; seen.messages.push(data.join('\r\n')); data = []; sock.write('250 queued\r\n'); }
          else data.push(line);
          continue;
        }
        seen.lines.push(line);
        if (line.startsWith('EHLO')) sock.write('250-fake\r\n250-AUTH PLAIN\r\n250 SIZE 1000\r\n');
        else if (line.startsWith('AUTH PLAIN ')) {
          const [, user, pass] = Buffer.from(line.slice(11), 'base64').toString('utf8').split('\0');
          sock.write(pass === password && user === 'owner@example.com' ? '235 ok\r\n' : '535 bad\r\n');
        } else if (line.startsWith('MAIL FROM') || line.startsWith('RCPT TO') || line === 'RSET') sock.write('250 ok\r\n');
        else if (line === 'DATA') { inData = true; sock.write('354 go\r\n'); }
        else if (line === 'QUIT') { sock.write('221 bye\r\n'); sock.end(); }
        else sock.write('500 what\r\n');
      }
    });
  });
  return new Promise(r => server.listen(0, '127.0.0.1', () => r({ server, seen, port: server.address().port })));
}

const connect = port => new Promise(r => { const s = net.connect(port, '127.0.0.1', () => r(nodeConn(s))); });

test('logs in, sends two messages on one connection, and quits', async () => {
  const { server, seen, port } = await fakeServer('app pass');
  const c = new SmtpClient(await connect(port));
  await c.login('owner@example.com', 'app pass');
  const msg = buildMessage({ fromName: 'เกาะเบาะ Kohbor', from: 'owner@example.com', to: 'a@example.com', subject: 'ลิงก์ดาวน์โหลด', body: '.leading dot\nสวัสดี {name}', date: new Date(0), id: 'x1' });
  await c.send('owner@example.com', 'a@example.com', msg);
  await c.reset();
  await c.send('owner@example.com', 'b@example.com', msg);
  await c.quit();
  server.close();
  assert.equal(seen.messages.length, 2);
  assert.deepEqual(seen.lines.filter(l => l.startsWith('RCPT')), ['RCPT TO:<a@example.com>', 'RCPT TO:<b@example.com>']);
  const m = seen.messages[0];
  assert.match(m, /^From: =\?UTF-8\?B\?.+\?= <owner@example.com>/);
  assert.match(m, /\r\nContent-Transfer-Encoding: base64\r\n\r\n/);
  const body = Buffer.from(m.split('\r\n\r\n')[1].replace(/\r\n/g, ''), 'base64').toString('utf8');
  assert.equal(body, '.leading dot\r\nสวัสดี {name}');
});

test('wrong password raises an auth error', async () => {
  const { server, port } = await fakeServer('right');
  const c = new SmtpClient(await connect(port));
  await assert.rejects(c.login('owner@example.com', 'wrong'), e => e instanceof SmtpError && e.stage === 'auth' && e.code === 535);
  await c.quit(); server.close();
});

test('encoded subject decodes back and keeps words short', () => {
  const s = 'ลิงก์ดาวน์โหลดเกาะเบาะ (alpha) / Your Kohbor alpha download link';
  const words = encodeWords(s).split('\r\n ');
  assert.ok(words.every(w => w.length <= 75));
  const back = Buffer.concat(words.map(w => Buffer.from(/^=\?UTF-8\?B\?(.*)\?=$/.exec(w)[1], 'base64'))).toString('utf8');
  assert.equal(back, s);
  assert.equal(encodeWords('plain ascii'), 'plain ascii');
});

test('addresses that could inject headers or commands are refused', () => {
  for (const bad of ['a@b.c\r\nBcc: x@y.z', 'a b@c.d', '<a@b.c>', 'a@b', 'a@b.c>, x@y.z']) assert.equal(ADDRESS.test(bad), false, bad);
  assert.equal(ADDRESS.test('first.last+tag@gmail.com'), true);
  assert.throws(() => buildMessage({ fromName: 'x', from: 'o@e.com', to: 'a@b.c\r\nBcc: z@z.z', subject: 's', body: 'b', date: new Date(0), id: 'i' }));
});

test('template fills only the known fields', () => {
  assert.equal(fill('{name}|{email}|{link}|{other}', { name: 'N', email: 'E', link: 'L' }), 'N|E|L|{other}');
});

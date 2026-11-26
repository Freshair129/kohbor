// Minimal SMTP-over-TLS client and plain-text MIME builder for the invite email.
// Runs on Deno (Supabase Edge) and on Node 24 (tests); the TLS socket is passed in.
// Never logs credentials, addresses or message text.

export interface SmtpConn {
  read(p: Uint8Array): Promise<number | null>;
  write(p: Uint8Array): Promise<number>;
  close(): void;
}

export class SmtpError extends Error {
  stage: string;
  code: number;
  constructor(stage: string, code: number) { super(`smtp ${stage} ${code}`); this.stage = stage; this.code = code; }
}

const enc = new TextEncoder();
const dec = new TextDecoder();

export class SmtpClient {
  private buf = "";
  private conn: SmtpConn;
  private timeoutMs: number;
  constructor(conn: SmtpConn, timeoutMs = 20000) { this.conn = conn; this.timeoutMs = timeoutMs; }

  private async readLine(): Promise<string> {
    for (;;) {
      const i = this.buf.indexOf("\r\n");
      if (i >= 0) { const line = this.buf.slice(0, i); this.buf = this.buf.slice(i + 2); return line; }
      const chunk = new Uint8Array(4096);
      let timer: ReturnType<typeof setTimeout> | undefined;
      const n = await Promise.race([
        this.conn.read(chunk),
        new Promise<null>((_, reject) => { timer = setTimeout(() => reject(new SmtpError("timeout", 0)), this.timeoutMs); }),
      ]).finally(() => clearTimeout(timer));
      if (n === null || n === 0) throw new SmtpError("closed", 0);
      this.buf += dec.decode(chunk.subarray(0, n), { stream: true });
    }
  }

  async reply(): Promise<number> {
    for (;;) {
      const line = await this.readLine();
      const m = /^(\d{3})([ -])/.exec(line);
      if (!m) throw new SmtpError("garbled", 0);
      if (m[2] === " ") return Number(m[1]);
    }
  }

  private async write(text: string) {
    const bytes = enc.encode(text);
    let off = 0;
    while (off < bytes.length) off += await this.conn.write(bytes.subarray(off));
  }

  async cmd(stage: string, line: string, ok: number[]): Promise<number> {
    await this.write(line + "\r\n");
    const code = await this.reply();
    if (!ok.includes(code)) throw new SmtpError(stage, code);
    return code;
  }

  async login(user: string, pass: string) {
    const greet = await this.reply();
    if (greet !== 220) throw new SmtpError("greeting", greet);
    await this.cmd("ehlo", "EHLO localhost", [250]);
    const plain = btoa(String.fromCharCode(...enc.encode(`\0${user}\0${pass}`)));
    await this.cmd("auth", `AUTH PLAIN ${plain}`, [235]);
  }

  async send(from: string, to: string, message: string) {
    await this.cmd("mail", `MAIL FROM:<${from}>`, [250]);
    await this.cmd("rcpt", `RCPT TO:<${to}>`, [250, 251]);
    await this.cmd("data", "DATA", [354]);
    // Dot-stuffing; the body is base64 so this only matters for headers, but keep SMTP correct.
    const stuffed = message.split("\r\n").map(l => (l.startsWith(".") ? "." + l : l)).join("\r\n");
    await this.cmd("body", stuffed + "\r\n.", [250]);
  }

  async reset() { await this.cmd("rset", "RSET", [250]); }

  async quit() {
    try { await this.cmd("quit", "QUIT", [221]); } catch { /* closing anyway */ }
    try { this.conn.close(); } catch { /* already closed */ }
  }
}

// A plain address with no spaces, brackets or line breaks, so it cannot inject SMTP or header text.
export const ADDRESS = /^[^\s@<>,;:"()\[\]\\]+@[^\s@<>,;:"()\[\]\\]+\.[^\s@<>,;:"()\[\]\\]+$/;

function b64(bytes: Uint8Array): string {
  let s = "";
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
}

// RFC 2047 encoded words, split on character boundaries so each word stays under 76 characters.
export function encodeWords(text: string): string {
  if (/^[\x20-\x7e]*$/.test(text)) return text;
  const words: string[] = [];
  let part: number[] = [];
  for (const ch of text) {
    const bytes = Array.from(enc.encode(ch));
    if (part.length + bytes.length > 45) { words.push(part.slice()); part = []; }
    part.push(...bytes);
  }
  if (part.length) words.push(part);
  return words.map(w => `=?UTF-8?B?${b64(new Uint8Array(w))}?=`).join("\r\n ");
}

export function fill(template: string, vars: Record<string, string>): string {
  return template.replace(/\{(name|email|link)\}/g, (_, k: string) => vars[k] ?? "");
}

export function buildMessage(o: { fromName: string; from: string; to: string; subject: string; body: string; date: Date; id: string }): string {
  if (!ADDRESS.test(o.from) || !ADDRESS.test(o.to)) throw new Error("address");
  const body = b64(enc.encode(o.body.replace(/\r?\n/g, "\r\n"))).replace(/.{1,76}/g, "$&\r\n").trimEnd();
  const domain = o.from.split("@")[1];
  return [
    `From: ${encodeWords(o.fromName)} <${o.from}>`,
    `To: <${o.to}>`,
    `Subject: ${encodeWords(o.subject.replace(/[\r\n]+/g, " "))}`,
    `Date: ${o.date.toUTCString().replace("GMT", "+0000")}`,
    `Message-ID: <${o.id}@${domain}>`,
    "MIME-Version: 1.0",
    "Content-Type: text/plain; charset=UTF-8",
    "Content-Transfer-Encoding: base64",
    "",
    body,
  ].join("\r\n");
}

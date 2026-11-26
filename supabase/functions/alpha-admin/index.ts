// Kohbor alpha admin. POST with the caller's Supabase Auth access token and {action, ...}.
// Deploy with verify_jwt on. The caller comes only from the verified token; the database functions
// refuse anyone not listed in public.alpha_admins. Never logs the token, emails, links or user ids.
//   {action:"queue"}                                  -> {queue, links, template, mail_ready}
//   {action:"set_invited", users:[uuid], invited}     -> {changed}
//   {action:"set_template", subject, body}            -> {template}
//   {action:"test_mail"}                              -> {sent:true}   (template to the admin's own address)
//   {action:"invite", users:[uuid] (max 20)}          -> {sent:[uuid], failed:[uuid], changed}
// Mail goes out through Gmail SMTP (smtp.gmail.com:465) with the secrets KOHBOR_GMAIL_USER and
// KOHBOR_GMAIL_APP_PASSWORD, which the owner sets in the Supabase dashboard.
import { createClient } from "npm:@supabase/supabase-js@2.49.4";
import { ADDRESS, SmtpClient, SmtpError, buildMessage, fill } from "./mail.ts";

const ALLOWED_ORIGIN = /^https:\/\/(kohbor-site-prototype\.vercel\.app|kohbor-site-prototype-[a-z0-9]+-pornpons-projects\.vercel\.app)$/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const SITE = "https://kohbor-site-prototype.vercel.app";
const FROM_NAME = "เกาะเบาะ Kohbor";
const MAX_INVITES = 20;

function cors(origin: string | null): Record<string, string> {
  const h: Record<string, string> = { "Vary": "Origin", "Content-Type": "application/json", "Cache-Control": "no-store" };
  if (origin && ALLOWED_ORIGIN.test(origin)) {
    h["Access-Control-Allow-Origin"] = origin;
    h["Access-Control-Allow-Methods"] = "POST, OPTIONS";
    h["Access-Control-Allow-Headers"] = "authorization, apikey, content-type";
  }
  return h;
}

function reply(status: number, body: unknown, origin: string | null) {
  return new Response(JSON.stringify(body), { status, headers: cors(origin) });
}

function failed(error: { code?: string } | null, origin: string | null) {
  if (error?.code === "42501") return reply(403, { error: "forbidden" }, origin);
  if (error?.code === "22023") return reply(400, { error: "bad_request" }, origin);
  return reply(503, { error: "unavailable" }, origin);
}

function mailAccount(): { user: string; pass: string } | null {
  const user = (Deno.env.get("KOHBOR_GMAIL_USER") ?? "").trim();
  const pass = (Deno.env.get("KOHBOR_GMAIL_APP_PASSWORD") ?? "").replace(/\s+/g, "");
  return ADDRESS.test(user) && pass ? { user, pass } : null;
}

async function openMail(account: { user: string; pass: string }): Promise<SmtpClient> {
  const conn = await Deno.connectTls({ hostname: "smtp.gmail.com", port: 465 });
  const client = new SmtpClient(conn);
  try { await client.login(account.user, account.pass); } catch (e) { await client.quit(); throw e; }
  return client;
}

function mailError(e: unknown, origin: string | null) {
  const login = e instanceof SmtpError && (e.stage === "auth" || e.stage === "greeting" || e.stage === "ehlo");
  return reply(502, { error: login ? "mail_login" : "mail_failed" }, origin);
}

function newKey(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(32));
  return btoa(String.fromCharCode(...bytes)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

async function sha256Hex(text: string): Promise<string> {
  const d = new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text)));
  return Array.from(d, b => b.toString(16).padStart(2, "0")).join("");
}

Deno.serve(async (req: Request) => {
  const origin = req.headers.get("origin");
  if (req.method === "OPTIONS") return new Response(null, { status: 204, headers: cors(origin) });
  if (req.method !== "POST") return reply(405, { error: "method" }, origin);
  const auth = req.headers.get("authorization") ?? "";
  const token = auth.startsWith("Bearer ") ? auth.slice(7) : "";
  if (!token) return reply(401, { error: "signin" }, origin);

  let body: Record<string, unknown>;
  try { body = await req.json(); } catch { return reply(400, { error: "bad_request" }, origin); }
  if (!body || typeof body !== "object") return reply(400, { error: "bad_request" }, origin);

  const db = createClient(Deno.env.get("SUPABASE_URL")!, Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!, { auth: { persistSession: false } });
  const { data: who, error: authError } = await db.auth.getUser(token);
  if (authError || !who?.user?.id) return reply(401, { error: "signin" }, origin);
  const admin = who.user.id;

  if (body.action === "queue") {
    const q = await db.rpc("alpha_admin_queue", { p_admin: admin });
    if (q.error) return failed(q.error, origin);
    const l = await db.rpc("alpha_admin_links", { p_admin: admin });
    if (l.error) return failed(l.error, origin);
    const t = await db.rpc("alpha_admin_get_template", { p_admin: admin });
    if (t.error) return failed(t.error, origin);
    return reply(200, { queue: q.data ?? [], links: l.data ?? [], template: (t.data ?? [])[0] ?? null, mail_ready: mailAccount() !== null }, origin);
  }

  if (body.action === "set_invited") {
    const users = body.users;
    if (!Array.isArray(users) || !users.length || users.length > 500 || !users.every(u => typeof u === "string" && UUID.test(u)) || typeof body.invited !== "boolean") {
      return reply(400, { error: "bad_request" }, origin);
    }
    const r = await db.rpc("alpha_admin_set_invited", { p_admin: admin, p_users: users, p_invited: body.invited });
    if (r.error) return failed(r.error, origin);
    return reply(200, { changed: r.data ?? [] }, origin);
  }

  if (body.action === "set_template") {
    const { subject, body: text } = body as { subject?: unknown; body?: unknown };
    if (typeof subject !== "string" || typeof text !== "string" || !subject.trim() || subject.length > 200 || !text.includes("{link}") || text.length > 10000) {
      return reply(400, { error: "bad_request" }, origin);
    }
    const r = await db.rpc("alpha_admin_set_template", { p_admin: admin, p_subject: subject.trim(), p_body: text });
    if (r.error) return failed(r.error, origin);
    return reply(200, { template: (r.data ?? [])[0] ?? null }, origin);
  }

  if (body.action === "test_mail" || body.action === "invite") {
    // Check admin rights before touching the mailbox.
    const t = await db.rpc("alpha_admin_get_template", { p_admin: admin });
    if (t.error) return failed(t.error, origin);
    const template = (t.data ?? [])[0] as { subject: string; body: string } | undefined;
    if (!template) return reply(503, { error: "unavailable" }, origin);
    const account = mailAccount();
    if (!account) return reply(503, { error: "mail_not_configured" }, origin);

    if (body.action === "test_mail") {
      const to = who.user.email ?? "";
      if (!ADDRESS.test(to)) return reply(400, { error: "bad_request" }, origin);
      const name = (who.user.user_metadata?.full_name ?? who.user.user_metadata?.name ?? "") as string;
      const vars = { name, email: to, link: `${SITE}/get.html#sample-link-not-a-real-key` };
      let client: SmtpClient;
      try { client = await openMail(account); } catch (e) { return mailError(e, origin); }
      try {
        await client.send(account.user, to, buildMessage({ fromName: FROM_NAME, from: account.user, to, subject: "[TEST] " + fill(template.subject, vars), body: fill(template.body, vars), date: new Date(), id: crypto.randomUUID() }));
      } catch (e) { await client.quit(); return mailError(e, origin); }
      await client.quit();
      return reply(200, { sent: true }, origin);
    }

    const users = body.users;
    if (!Array.isArray(users) || !users.length || users.length > MAX_INVITES || !users.every(u => typeof u === "string" && UUID.test(u))) {
      return reply(400, { error: "bad_request" }, origin);
    }
    const r = await db.rpc("alpha_admin_recipients", { p_admin: admin, p_users: users });
    if (r.error) return failed(r.error, origin);
    const people = (r.data ?? []) as { user_id: string; email: string; name: string | null }[];

    let client: SmtpClient;
    try { client = await openMail(account); } catch (e) { return mailError(e, origin); }
    const sent: string[] = [];
    const failedIds: string[] = users.filter(u => !people.some(p => p.user_id === u)) as string[];
    for (const p of people) {
      if (!ADDRESS.test(p.email)) { failedIds.push(p.user_id); continue; }
      const key = newKey();
      const issued = await db.rpc("alpha_admin_issue_link", { p_admin: admin, p_user: p.user_id, p_link_hash: await sha256Hex(key) });
      const linkId = (issued.data ?? [])[0]?.id;
      if (issued.error || linkId == null) { failedIds.push(p.user_id); continue; }
      const vars = { name: p.name ?? "", email: p.email, link: `${SITE}/get.html#${key}` };
      try {
        if (sent.length) await client.reset();
        await client.send(account.user, p.email, buildMessage({ fromName: FROM_NAME, from: account.user, to: p.email, subject: fill(template.subject, vars), body: fill(template.body, vars), date: new Date(), id: crypto.randomUUID() }));
        sent.push(p.user_id);
      } catch {
        await db.rpc("alpha_admin_revoke_link", { p_admin: admin, p_link_id: linkId });
        failedIds.push(p.user_id);
        // A broken connection fails every later send too; stop and report the rest as not sent.
        for (const rest of people.slice(people.indexOf(p) + 1)) failedIds.push(rest.user_id);
        break;
      }
    }
    await client.quit();
    let changed: unknown[] = [];
    if (sent.length) {
      const m = await db.rpc("alpha_admin_set_invited", { p_admin: admin, p_users: sent, p_invited: true });
      changed = m.data ?? [];
    }
    return reply(200, { sent, failed: failedIds, changed }, origin);
  }

  return reply(400, { error: "bad_request" }, origin);
});
